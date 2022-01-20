use crate::attr::Attributes;
use crate::utils::err;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::AttributeArgs;
use syn::Error;
use syn::Expr;
use syn::Item;
use syn::ItemFn;
use syn::Result;
use syn::Token;
use syn::Type;

#[allow(unused_macros)]
macro_rules! ugly_prefix {
    () => {
        __crayfish_macro_helper_function__
    };
}

fn prepend_ugly_prefix(suffix: &str) -> TokenStream {
    let ident: TokenStream = format!("__crayfish_d81432540815a7cb_{}", suffix)
        .parse()
        .unwrap();
    quote!(#ident)
}

fn context_arg_name() -> TokenStream {
    prepend_ugly_prefix("arg_ctx")
}

fn at_async_fn_name(fn_name: &TokenStream) -> TokenStream {
    prepend_ugly_prefix(&format!("at_async_{}", fn_name))
}

fn at_ff_fn_name(fn_name: &TokenStream) -> TokenStream {
    prepend_ugly_prefix(&format!("at_ff_{}", fn_name))
}

struct HelperFunctionsGenerator {
    crayfish_path: TokenStream,
    fn_id: TokenStream,
    fn_name: TokenStream,
    params: Vec<(String, Type)>,
    pub ret_type: TokenStream,
}

impl HelperFunctionsGenerator {
    fn infer_ret_by_future(ret_ty: &Type) -> Result<TokenStream> {
        const INFER_ERR_MSG: &str =
            "can not infer return type. Please use 'async' keyword or set a attribute: #[activity(ret = \"Type\")]";

        let ret = match ret_ty {
            syn::Type::Path(p) => {
                // only match BoxFuture<'a, ret_type>
                if p.qself.is_none() && p.path.segments.len() == 1 {
                    let box_fut_t = p.path.segments.last().unwrap();
                    if &format!("{}", box_fut_t.ident) == "BoxFuture" {
                        if let syn::PathArguments::AngleBracketed(ref pargs) = box_fut_t.arguments {
                            if pargs.args.len() == 2 {
                                if let syn::GenericArgument::Type(t) = pargs.args.last().unwrap() {
                                    return Ok(quote!(#t));
                                }
                            }
                        }
                    }
                }
                err(ret_ty, INFER_ERR_MSG)?
            }
            syn::Type::ImplTrait(p) => {
                // only match impl Future<Output=ret>
                if p.bounds.len() == 1 {
                    if let syn::TypeParamBound::Trait(t) = p.bounds.last().unwrap() {
                        if t.path.segments.len() == 1 {
                            let output = t.path.segments.last().unwrap();
                            let trait_ident = &output.ident;
                            if &quote!(#trait_ident).to_string() == "Future" {
                                if let syn::PathArguments::AngleBracketed(ref pargs) =
                                    output.arguments
                                {
                                    if pargs.args.len() == 1 {
                                        if let syn::GenericArgument::Binding(b) =
                                            pargs.args.last().unwrap()
                                        {
                                            let ty = &b.ty;
                                            return Ok(quote!(#ty));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                err(ret_ty, INFER_ERR_MSG)?
            }
            _ => err(ret_ty, INFER_ERR_MSG)?,
        };

        Ok(ret)
    }
    fn infer_ret(function: &ItemFn) -> Result<TokenStream> {
        let ItemFn {
            sig: syn::Signature {
                output, asyncness, ..
            },
            ..
        } = function;

        let tk = match asyncness {
            Some(_) => match output {
                syn::ReturnType::Default => quote!(()),
                syn::ReturnType::Type(_, t) => quote!(#t),
            },
            None => match output {
                syn::ReturnType::Default => err(output, "should return a future")?,
                syn::ReturnType::Type(_, t) => Self::infer_ret_by_future(&**t)?,
            },
        };
        Ok(tk)
    }

    fn new(function: &ItemFn, crayfish_path: &TokenStream, attrs: &Attributes) -> Result<Self> {
        let crayfish_path = crayfish_path.clone();

        let ItemFn {
            sig: syn::Signature { ident, inputs, .. },
            ..
        } = function;

        let fn_name: TokenStream = quote!(#ident);
        let file: TokenStream = file!().parse().unwrap();
        let line: TokenStream = line!().to_string().parse().unwrap();
        let path: TokenStream = module_path!().parse().unwrap();
        let fn_id: TokenStream = fn_hash(&fn_name, &file, &line, &path)
            .to_string()
            .parse()
            .unwrap();
        let ret_type: TokenStream = match &attrs.ret_type {
            Some(t) => quote!(#t),
            None => Self::infer_ret(function)?,
        };

        // first param is impl Context
        let params = inputs.clone().into_iter();
        let params: Vec<(String, Type)> = params
            .enumerate()
            .map(|(i, p)| match p {
                syn::FnArg::Typed(pt) => (format!("__crayfish_arg{}", i), *pt.ty),
                _ => panic!("method not implemented"),
            })
            .collect();
        Ok(HelperFunctionsGenerator {
            crayfish_path,
            fn_id,
            fn_name,
            params,
            ret_type,
        })
    }

    fn punctuated_params(&self) -> TokenStream {
        let ps: Vec<_> = self
            .params
            .iter()
            .map(|(ref ident, ref ty)| {
                format!("{}:{}", ident, quote!(#ty))
                    .parse::<TokenStream>()
                    .unwrap()
            })
            .collect();
        quote!(#(#ps),*)
    }

    fn param_ident_list(&self) -> Vec<TokenStream> {
        self.params
            .iter()
            .map(|(ref ident, _)| ident.parse::<TokenStream>().unwrap())
            .collect()
    }

    fn handler_fn_name(&self) -> TokenStream {
        prepend_ugly_prefix(&format!("handler_{}", self.fn_name))
    }

    fn execute_fn_name(&self) -> TokenStream {
        prepend_ugly_prefix(&format!("execute_{}", self.fn_name))
    }

    fn gen_at_ff(&self) -> TokenStream {
        let crayfish_path = &self.crayfish_path;
        let fn_id = &self.fn_id;
        let at_ff_fn_name = at_ff_fn_name(&self.fn_name);
        let execute_fn_name = self.execute_fn_name();
        let punctuated_params = self.punctuated_params();
        let param_ident_list = self.param_ident_list();
        let profiling_label =
            syn::LitStr::new(&format!("{}_ff_serialize", self.fn_name), Span::call_site());

        quote! {

        fn #at_ff_fn_name(
            a_id: #crayfish_path::activity::ActivityId,
            dst_place: #crayfish_path::place::Place,
            #punctuated_params
        ) {
            let fn_id = #fn_id; // macro

            if dst_place == #crayfish_path::place::here() {
                #crayfish_path::spawn(#execute_fn_name(a_id, true, #(#param_ident_list),*)); // macro
            } else {
                // trace!("spawn activity:{} at place: {}", a_id, dst_place);
                let mut builder = #crayfish_path::activity::TaskItemBuilder::new(fn_id, dst_place, a_id);

                #crayfish_path::profiling_start!(#profiling_label);
                #(builder.arg(#param_ident_list);)*
                #crayfish_path::profiling_stop!();

                let item = builder.build_box();
                use #crayfish_path::runtime::ApgasContext;
                #crayfish_path::runtime::ConcreteContext::send(item);
            }
        }

        }
    }

    fn gen_at_async(&self) -> TokenStream {
        let crayfish_path = &self.crayfish_path;
        let fn_id = &self.fn_id;
        let ret_type = &self.ret_type;
        let at_async_fn_name = at_async_fn_name(&self.fn_name);
        let execute_fn_name = self.execute_fn_name();
        let punctuated_params = self.punctuated_params();
        let param_ident_list = self.param_ident_list();
        let profiling_label = syn::LitStr::new(
            &format!("{}_async_serialize", self.fn_name),
            Span::call_site(),
        );

        quote! {

        fn #at_async_fn_name(
            a_id: #crayfish_path::activity::ActivityId,
            dst_place: #crayfish_path::place::Place,
            #punctuated_params
        ) -> impl #crayfish_path::re_export::futures::Future<Output = #ret_type > {
            let fn_id = #fn_id; // macro

            let f = #crayfish_path::runtime::wait_single::<#ret_type>(a_id); // macro
            if dst_place == #crayfish_path::place::here() {
                #crayfish_path::spawn(#execute_fn_name(a_id, true, #(#param_ident_list),*)); // macro
            } else {
                // trace!("spawn activity:{} at place: {}", a_id, dst_place);
                let mut builder = #crayfish_path::activity::TaskItemBuilder::new(fn_id, dst_place, a_id);

                #crayfish_path::profiling_start!(#profiling_label);
                #(builder.arg(#param_ident_list);)*
                #crayfish_path::profiling_stop!();

                builder.waited();
                let item = builder.build_box();
                use #crayfish_path::runtime::ApgasContext;
                #crayfish_path::runtime::ConcreteContext::send(item);
            }
            f
        }

        }
    }

    fn gen_execute(&self) -> TokenStream {
        let crayfish_path = &self.crayfish_path;
        let fn_id = &self.fn_id;
        let fn_name = &self.fn_name;
        let execute_fn_name = self.execute_fn_name();
        let punctuated_params = self.punctuated_params();
        let param_ident_list = self.param_ident_list();

        quote! {
        async fn #execute_fn_name(a_id: #crayfish_path::activity::ActivityId, waited: ::std::primitive::bool, #punctuated_params) {
            let fn_id = #fn_id; // macro
            use #crayfish_path::re_export::futures::FutureExt;
            let finish_id = a_id.get_finish_id();
            use #crayfish_path::runtime::ApgasContext;
            let mut ctx = #crayfish_path::runtime::ConcreteContext::inherit(finish_id);
            // ctx seems to be unwind safe
            let future = ::std::panic::AssertUnwindSafe(#fn_name(&mut ctx, #(#param_ident_list),* )); //macro
            let result = future.catch_unwind().await;
            #crayfish_path::essence::send_activity_result(ctx, a_id, fn_id, waited, result);
        }
        }
    }

    fn gen_handler(&self) -> TokenStream {
        let crayfish_path = &self.crayfish_path;
        let fn_id = &self.fn_id;
        let handler_fn_name = self.handler_fn_name();
        let execute_fn_name = self.execute_fn_name();

        let extract_args =
            (0..self.params.len()).map(|i| format!("arg{}", i).parse::<TokenStream>().unwrap());
        let arg_stmts = (0..self.params.len())
            .map(|i| syn::parse_str::<syn::Stmt>(&format!("let arg{} = e.arg();", i)).unwrap());
        let profiling_label = syn::LitStr::new(
            &format!("{}_deserialization", self.fn_name),
            Span::call_site(),
        );

        quote! {

        fn #handler_fn_name(item: #crayfish_path::activity::TaskItem) -> #crayfish_path::re_export::futures::future::BoxFuture<'static, ()> {
        use #crayfish_path::re_export::futures::FutureExt;
        async move {
            let waited = item.is_waited();
            let mut e = #crayfish_path::activity::TaskItemExtracter::new(item);
            let a_id = e.activity_id();

            // wait until function return
            // #crayfish_path::logging::trace!(
            //     "Got activity:{} from {}", a_id, a_id.get_spawned_place()
            // );
            #crayfish_path::profiling_start!(#profiling_label);
            #(#arg_stmts)*
            #crayfish_path::profiling_stop!();
            #execute_fn_name(a_id, waited, #(#extract_args),*).await;
        }
        .boxed()
        }

        // register function
        const _:() = {
            use #crayfish_path::inventory;
            #crayfish_path::inventory::submit! {
                #crayfish_path::runtime_meta::FunctionMetaData::new(
                    #fn_id,
                    #handler_fn_name,
                    ::std::string::String::from("basic"),
                    ::std::string::String::from(::std::file!()),
                    ::std::line!(),
                    ::std::string::String::from(::std::module_path!())
                )
            };
        };

        }
    }
}

fn _expand_async_func(attrs: Attributes, function: ItemFn) -> Result<TokenStream> {
    // TODO: support re-export crayfish
    //

    let crayfish_path: TokenStream = attrs.get_path();
    let gen = HelperFunctionsGenerator::new(&function, &crayfish_path, &attrs)?;

    let execute_fn = gen.gen_execute();
    let handler_fn = gen.gen_handler();
    let at_async_fn = gen.gen_at_async();
    let at_ff_fn = gen.gen_at_ff();

    let mut function = function;
    // modify fn
    let ItemFn {
        ref mut sig,
        ref mut block,
        ..
    } = function;

    let context_arg_name = context_arg_name();
    let arg_token;

    // change to boxed
    if sig.asyncness.is_some() {
        sig.asyncness = None;
        let ret_type = &gen.ret_type;
        sig.output = syn::parse2(
            quote!( -> #crayfish_path::re_export::futures::future::BoxFuture<'cfctxlt, #ret_type> ),
        )?;
        arg_token =
            quote!(#context_arg_name: &'cfctxlt mut impl #crayfish_path::runtime::ApgasContext);
        sig.generics = syn::parse2(quote!(<'cfctxlt>))?;
        *block = Box::new(syn::parse2(quote! {
            {
                use #crayfish_path::re_export::futures::FutureExt;
                async move #block .boxed()
            }
        })?)
    } else {
        arg_token = quote!(#context_arg_name: &mut impl #crayfish_path::runtime::ApgasContext);
    }

    // insert context
    let context_arg: syn::FnArg = syn::parse2(arg_token)?;
    sig.inputs.insert(0, context_arg);

    Ok(quote!(
    #function

    #execute_fn

    #handler_fn

    #at_async_fn

    #at_ff_fn
    ))
}

pub(crate) fn expand_async_func(attrs: Attributes, item: Item) -> Result<TokenStream> {
    if let Item::Fn(function) = item {
        verify_func(&function)?;
        _expand_async_func(attrs, function)
    } else {
        Err(Error::new_spanned(item, "only support function item"))
    }
}

// TODO: support method
fn verify_func(func: &ItemFn) -> Result<()> {
    let generics = &func.sig.generics;
    if !func.sig.inputs.is_empty() {
        let first_arg = &func.sig.inputs.first().unwrap();
        if let syn::FnArg::Receiver(_) = first_arg {
            return Err(Error::new_spanned(
                first_arg,
                "currently doesn't support method",
            ));
        }
    }

    if func.sig.variadic.is_some() {
        return Err(Error::new_spanned(
            &func.sig.variadic,
            "Crayfish doesn't support variadic functions",
        ));
    }
    if !generics.params.is_empty() {
        Err(Error::new_spanned(
            generics.clone(),
            "Crayfish currently doesn't support generics",
        ))
    } else {
        Ok(())
    }
}

fn fn_hash(
    fn_name: &TokenStream,
    file: &TokenStream,
    line: &TokenStream,
    path: &TokenStream,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut hasher = DefaultHasher::new();
    let s = quote! { #fn_name #file #line #path }.to_string();
    hasher.write(s.as_bytes());
    hasher.finish()
}

pub enum SpawnMethod {
    At,
    FireAndForget,
}

pub fn expand_at(input: proc_macro::TokenStream, spawn: SpawnMethod) -> Result<TokenStream> {
    let parser = Punctuated::<Expr, Token![,]>::parse_separated_nonempty;
    use syn::parse::Parser;
    let args = parser.parse(input)?;

    let mut args = args;
    // check args num
    let expected_arg_num: usize = 2;
    if args.len() != expected_arg_num {
        err(
            &args,
            format!(
                "this macro takes {} argument but {} arguments were supplied",
                expected_arg_num,
                args.len()
            ),
        )?;
    }

    // get func name & call args
    let call = args.pop().unwrap().into_value();
    let (async_func_name, call_args) =
        match call {
            Expr::Call(syn::ExprCall {
                attrs, func, args, ..
            }) => {
                if !attrs.is_empty() {
                    err(&attrs[0], "Crayfish doesn't suport attribute(s) here")?;
                }
                let func_name = match *func {
                    Expr::Path(p) => {
                        let mut p = p;
                        let mut last = p.path.segments.pop().unwrap().into_value();
                        if !last.arguments.is_empty() {
                            err(&last, "Crayfish doesn't support generic function")?;
                        }
                        let last_ident = &last.ident;
                        let last_ident_str = match spawn {
                            SpawnMethod::At => at_async_fn_name,
                            SpawnMethod::FireAndForget => at_ff_fn_name,
                        }(&quote!(#last_ident))
                        .to_string();
                        last.ident = syn::Ident::new(last_ident_str.as_str(), last.ident.span());
                        p.path.segments.push(last);
                        p
                    }
                    thing => return err(thing, "must be a proper function name"),
                };
                (func_name, args)
            }
            Expr::MethodCall(_) => return err(&call, "haven't support method call yet."),
            _ => return err(
                &call,
                "the second argument must be call-like expression: \"func_name(arg0, arg1, ..)\"",
            ),
        };

    let place = args.pop();
    let context_arg_name = context_arg_name();

    let ret = quote! {#async_func_name(#context_arg_name.spawn(), #place #call_args)};
    Ok(ret)
}

pub fn finish(args: Option<AttributeArgs>, input: proc_macro::TokenStream) -> Result<TokenStream> {
    let attrs = match args {
        Some(args) => Attributes::new(args)?,
        None => Attributes::default(),
    };

    let block = TokenStream::from(input);

    // error if return inside finish block
    for tree in block.clone().into_iter() {
        match tree {
            TokenTree::Ident(id) => {
                if &id.to_string() == "return" {
                    return err(id, "returning from finish blocks is not allowed until the async closure become stable in Rust");
                }
            }
            TokenTree::Punct(p) => {
                if p.as_char() == '?' {
                    return err(p, "try expression is not allowed in finish block. TODO: this check might be false positive.");
                }
            }
            _ => (),
        }
    }

    let crayfish_path = attrs.get_path();
    let context_arg_name = context_arg_name();

    let ret = quote! {
        {
        use crayfish::runtime::ApgasContext;
        let mut #context_arg_name = #crayfish_path::runtime::ConcreteContext::new_frame();
        let _block_ret = {
            #block
        };
        #crayfish_path::runtime::wait_all(#context_arg_name).await;
        _block_ret
        }
    };
    Ok(ret)
}

pub fn main(args: AttributeArgs, main: ItemFn) -> Result<TokenStream> {
    let attrs = Attributes::new(args)?;
    let crayfish_path = attrs.get_path();

    if main.sig.asyncness.is_none() {
        return err(&main.sig, "Crayfish requires main function to be 'async'");
    }

    let mut main = main;
    // change main ident

    // check args. if empty, insert
    if main.sig.inputs.is_empty() {
        let arg = syn::parse2::<syn::FnArg>(quote!(_: ::std::vec::Vec<::std::string::String>))?;
        let args: Punctuated<syn::FnArg, Token![,]> = vec![arg].into_iter().collect();
        main.sig.inputs = args
    }

    // rename func
    let user_main_name = &main.sig.ident;
    let user_main_name = prepend_ugly_prefix(quote!(#user_main_name).to_string().as_str());
    main.sig.ident = syn::Ident::new(&user_main_name.to_string(), main.sig.ident.span());

    let output = &main.sig.output;

    let ret = quote!(

        #main

        pub fn main() #output{
            #crayfish_path::essence::genesis(#user_main_name)
        }

    );

    Ok(ret)
}
