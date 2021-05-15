use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::AttributeArgs;
use syn::Item;

mod args;
mod attr;
mod func;

extern crate proc_macro;

#[proc_macro_attribute]
pub fn arg(_args: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Item);
    args::impl_remote_send(args::RSendImpl::DefaultImpl, item).into()
}

#[proc_macro_attribute]
pub fn arg_squashable(_args: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Item);
    args::impl_remote_send(args::RSendImpl::Squashable, item).into()
}

#[proc_macro_attribute]
pub fn arg_squashed(_args: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Item);
    args::impl_remote_send(args::RSendImpl::Squashed, item).into()
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
