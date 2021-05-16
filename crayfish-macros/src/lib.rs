use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::AttributeArgs;
use syn::Item;

mod args;
mod attr;
mod func;
mod utils;

extern crate proc_macro;

#[proc_macro_attribute]
pub fn arg(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let item = parse_macro_input!(item as Item);
    args::impl_remote_send(args::RSendImpl::DefaultImpl, args, item)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn arg_squashable(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let item = parse_macro_input!(item as Item);
    args::impl_remote_send(args::RSendImpl::Squashable, args, item)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn arg_squashed(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let item = parse_macro_input!(item as Item);
    args::impl_remote_send(args::RSendImpl::Squashed, args, item)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

fn _activity(args: AttributeArgs, item: Item) -> syn::Result<proc_macro2::TokenStream> {
    let attrs = attr::Attributes::new(args)?;
    func::expand_async_func(attrs, item)
}

#[proc_macro_attribute]
pub fn activity(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let item = parse_macro_input!(item as Item);
    _activity(args, item)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// at!(place, func(a, b, c, d));
#[proc_macro]
pub fn at(input: TokenStream) -> TokenStream {
    func::expand_at(input, func::SpawnMethod::At)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// ff!(place, func(a, b, c, d));
#[proc_macro]
pub fn ff(input: TokenStream) -> TokenStream {
    func::expand_at(input, func::SpawnMethod::FireAndForget)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn finish_attr(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    func::finish(Some(args), input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro]
pub fn finish(input: TokenStream) -> TokenStream {
    func::finish(None, input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
