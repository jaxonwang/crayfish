use proc_macro2::TokenStream;
use quote::ToTokens;
use std::fmt::Display;
use syn::Error;

pub fn err(tokens: impl ToTokens, message: impl Display) -> syn::Result<TokenStream> {
    Err(Error::new_spanned(tokens, message))
}
