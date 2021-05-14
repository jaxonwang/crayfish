use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::Item;

mod args;

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
