use crate::attr::Attributes;
use proc_macro2::TokenStream;
use quote::quote;
use quote::ToTokens;
use std::fmt::Display;
use syn::punctuated::Punctuated;
use syn::Error;
use syn::Expr;
use syn::Item;
use syn::ItemFn;
use syn::Result;
use syn::Token;
use syn::Type;

fn err(tokens: impl ToTokens, message: impl Display) -> syn::Result<TokenStream> {
    Err(Error::new_spanned(tokens, message))
}

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
    ret_type: TokenStream,
}

impl HelperFunctionsGenerator {
    fn infer_ret_by_future(ret_ty: &Type) -> Result<TokenStream> {
        const INFER_ERR_MSG: &str =
            "can not infer return type. Please set attribute: #[activity(ret = \"Type\")]";

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

    fn new(function: &ItemFn, crayfish_path: &TokenStream) -> Result<Self> {
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
        let ret_type: TokenStream = Self::infer_ret(&function)?;

        // first param is impl Context
        let params: Vec<_> = inputs.clone().into_iter().collect();
        let params: Vec<(String, Type)> = params
            .into_iter()
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

        quote! {

        fn #at_ff_fn_name(
            a_id: #crayfish_path::activity::ActivityId,
            dst_place: #crayfish_path::place::Place,
            #punctuated_params
        ) {
            let fn_id = #fn_id; // macro

            if dst_place == #crayfish_path::global_id::here() {
                #crayfish_path::spawn(#execute_fn_name(a_id, true, #(#param_ident_list),*)); // macro
            } else {
                // trace!("spawn activity:{} at place: {}", a_id, dst_place);
                let mut builder = #crayfish_path::activity::TaskItemBuilder::new(fn_id, dst_place, a_id);
                #(builder.arg(#param_ident_list);)*
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

        quote! {

        fn #at_async_fn_name(
            a_id: #crayfish_path::activity::ActivityId,
            dst_place: #crayfish_path::place::Place,
            #punctuated_params
        ) -> impl #crayfish_path::futures::Future<Output = #ret_type > {
            let fn_id = #fn_id; // macro

            let f = #crayfish_path::runtime::wait_single::<#ret_type>(a_id); // macro
            if dst_place == #crayfish_path::global_id::here() {
                #crayfish_path::spawn(#execute_fn_name(a_id, true, #(#param_ident_list),*)); // macro
            } else {
                // trace!("spawn activity:{} at place: {}", a_id, dst_place);
                let mut builder = #crayfish_path::activity::TaskItemBuilder::new(fn_id, dst_place, a_id);
                #(builder.arg(#param_ident_list);)*
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
            use #crayfish_path::global_id::ActivityIdMethods;
            use #crayfish_path::futures::FutureExt;
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

        let extract_args = self
            .params
            .iter()
            .map(|_| "e.arg()".parse::<TokenStream>().unwrap());

        quote! {

        fn #handler_fn_name(item: #crayfish_path::activity::TaskItem) -> #crayfish_path::futures::future::BoxFuture<'static, ()> {
        use #crayfish_path::futures::FutureExt;
        async move {
            let waited = item.is_waited();
            let mut e = #crayfish_path::activity::TaskItemExtracter::new(item);
            let a_id = e.activity_id();

            // wait until function return
            use #crayfish_path::global_id::ActivityIdMethods;
            // #crayfish_path::logging::trace!(
            //     "Got activity:{} from {}", a_id, a_id.get_spawned_place()
            // );
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

    let crayfish_path: TokenStream = match attrs.crayfish_path {
        Some(p) => quote!(#p),
        None => "::crayfish".parse().unwrap(),
    };
    let gen = HelperFunctionsGenerator::new(&function, &crayfish_path)?;

    let execute_fn = gen.gen_execute();
    let handler_fn = gen.gen_handler();
    let at_async_fn = gen.gen_at_async();
    let at_ff_fn = gen.gen_at_ff();

    // insert context
    let context_arg_name = context_arg_name();
    let arg_token = quote!(#context_arg_name: &mut impl #crayfish_path::runtime::ApgasContext);
    let context_arg: syn::FnArg = syn::parse2(arg_token)?;
    let mut function = function;
    function.sig.inputs.insert(0, context_arg);

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
                let func_name;
                match *func {
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
                        func_name = p
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
