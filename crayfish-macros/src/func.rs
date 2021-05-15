use proc_macro2::TokenStream;
use quote::quote;
use syn::Error;
use syn::Item;
use syn::ItemFn;
use syn::Result;
use syn::Type;

#[allow(unused_macros)]
macro_rules! ugly_prefix {
    () => {
        __crayfish_macro_helper_function__
    };
}

fn prepend_ugly_prefix(suffix: &str) -> TokenStream {
    let ident: TokenStream = format!("__crayfish_macro_helper_function__{}", suffix)
        .parse()
        .unwrap();
    quote!(#ident)
}

fn execute_fn_name(fn_name: &TokenStream) -> TokenStream {
    prepend_ugly_prefix(&format!("execute_{}", fn_name))
}

fn gen_execute(
    crayfish_path: &TokenStream,
    fn_id: &TokenStream,
    fn_name: &TokenStream,
    params: &[(String, Type)],
) -> TokenStream {
    let execute_fn_name = execute_fn_name(fn_name);
    let punctuated_params = punctuated_params(params);
    let param_ident_list = param_ident_list(params);

    quote! {
    async fn #execute_fn_name(a_id: #crayfish_path::activity::ActivityId, waited: ::std::primitive::bool, #punctuated_params) {
        let fn_id = #fn_id; // macro
        use #crayfish_path::global_id::ActivityIdMethods;
        use #crayfish_path::futures::FutureExt;
        let finish_id = a_id.get_finish_id();
        let mut ctx = #crayfish_path::runtime::ConcreteContext::inherit(finish_id);
        // ctx seems to be unwind safe
        let future = ::std::panic::AssertUnwindSafe(#fn_name(&mut ctx, #(#param_ident_list),* )); //macro
        let result = future.catch_unwind().await;
        #crayfish_path::essence::send_activity_result(ctx, a_id, fn_id, waited, result);
    }
    }
}

fn handler_fn_name(fn_name: &TokenStream) -> TokenStream {
    prepend_ugly_prefix(&format!("handler_{}", fn_name))
}

fn gen_handler(
    crayfish_path: &TokenStream,
    fn_id: &TokenStream,
    fn_name: &TokenStream,
    params: &[(String, Type)],
) -> TokenStream {
    let handler_fn_name = handler_fn_name(fn_name);
    let execute_fn_name = execute_fn_name(fn_name);

    let extract_args = params
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

fn at_async_fn_name(fn_name: &TokenStream) -> TokenStream {
    prepend_ugly_prefix(&format!("at_async_{}", fn_name))
}

fn gen_at_async(
    crayfish_path: &TokenStream,
    fn_id: &TokenStream,
    fn_name: &TokenStream,
    params: &[(String, Type)],
    ret_type: &TokenStream,
) -> TokenStream {
    let at_async_fn_name = at_async_fn_name(fn_name);
    let execute_fn_name = execute_fn_name(fn_name);
    let punctuated_params = punctuated_params(params);
    let param_ident_list = param_ident_list(params);

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
            #crayfish_path::runtime::ConcreteContext::send(item);
        }
        f
    }

    }
}

fn at_ff_fn_name(fn_name: &TokenStream) -> TokenStream {
    prepend_ugly_prefix(&format!("at_ff_{}", fn_name))
}

fn gen_at_ff(
    crayfish_path: &TokenStream,
    fn_id: &TokenStream,
    fn_name: &TokenStream,
    params: &[(String, Type)],
) -> TokenStream {
    let at_ff_fn_name = at_ff_fn_name(fn_name);
    let execute_fn_name = execute_fn_name(fn_name);
    let punctuated_params = punctuated_params(params);
    let param_ident_list = param_ident_list(params);

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
            #crayfish_path::runtime::ConcreteContext::send(item);
        }
    }

    }
}

fn _expand_async_func(function: ItemFn) -> Result<TokenStream> {
    // TODO: support re-export crayfish
    let crayfish_path: TokenStream = "::crayfish".parse().unwrap();

    let ItemFn {
        sig:
            syn::Signature {
                ident,
                inputs,
                output,
                ..
            },
        ..
    } = &function;

    let fn_name: TokenStream = quote!(#ident);
    let file: TokenStream = file!().parse().unwrap();
    let line: TokenStream = line!().to_string().parse().unwrap();
    let path: TokenStream = module_path!().parse().unwrap();
    let fn_id: TokenStream = fn_hash(&fn_name, &file, &line, &path)
        .to_string()
        .parse()
        .unwrap();
    let ret_type: TokenStream = match output {
        syn::ReturnType::Default => quote!(()),
        syn::ReturnType::Type(_, t) => quote!(#t),
    };

    // first param is impl Context
    let params: Vec<_> = inputs.clone().into_iter().skip(1).collect();
    let params: Vec<(String, Type)> = params
        .into_iter()
        .enumerate()
        .map(|(i, p)| match p {
            syn::FnArg::Typed(pt) => (format!("__crayfish_arg{}", i), *pt.ty),
            _ => panic!("method not implemented"),
        })
        .collect();

    let execute_fn = gen_execute(&crayfish_path, &fn_id, &fn_name, &params[..]);
    let handler_fn = gen_handler(&crayfish_path, &fn_id, &fn_name, &params[..]);
    let at_async_fn = gen_at_async(&crayfish_path, &fn_id, &fn_name, &params[..], &ret_type);
    let at_ff_fn = gen_at_ff(&crayfish_path, &fn_id, &fn_name, &params[..]);
    Ok(quote!(
    #function

    #execute_fn

    #handler_fn

    #at_async_fn

    #at_ff_fn
    ))
}

pub(crate) fn expand_async_func(item: Item) -> Result<TokenStream> {
    if let Item::Fn(function) = item {
        verify_func(&function)?;
        _expand_async_func(function)
    } else {
        Err(Error::new_spanned(item, "only support function item"))
    }
}

// TODO: support method
fn verify_func(func: &ItemFn) -> Result<()> {
    let generics = &func.sig.generics;
    if func.sig.inputs.is_empty() {
        return Err(Error::new_spanned(
            generics,
            "Activity function's first args is \"impl ApgasContex\'",
        ));
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

fn punctuated_params(params: &[(String, Type)]) -> TokenStream {
    let ps: Vec<_> = params
        .iter()
        .map(|(ref ident, ref ty)| {
            format!("{}:{}", ident, quote!(#ty))
                .parse::<TokenStream>()
                .unwrap()
        })
        .collect();
    quote!(#(#ps),*)
}

fn param_ident_list(params: &[(String, Type)]) -> Vec<TokenStream> {
    params
        .iter()
        .map(|(ref ident, _)| ident.parse::<TokenStream>().unwrap())
        .collect()
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
