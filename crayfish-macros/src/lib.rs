
use quote::quote;
use proc_macro::TokenStream;

extern crate proc_macro;
extern crate crayfish;

#[proc_macro_attribute]
pub fn crayfish(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[cfg(test)]
mod test{

    #[test]
    pub fn test_pkg_name(){
    }
}
