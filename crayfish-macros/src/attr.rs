use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use quote::ToTokens;
use syn::AttributeArgs;
use syn::Error;
use syn::Meta;
use syn::NestedMeta;
use syn::Path;

pub struct Attributes {
    pub crayfish_path: Option<Path>,
}

const BAD_ATTR: &str = "bad attribute";

fn bad_attr(tokens: impl ToTokens) -> syn::Result<()> {
    Err(Error::new_spanned(tokens, BAD_ATTR))
}

impl Attributes {
    pub fn new(args: AttributeArgs) -> syn::Result<Self> {
        let mut crayfish_path: Option<Path> = None;

        for arg in args {
            match arg {
                NestedMeta::Meta(Meta::NameValue(nv)) => {
                    let path = &nv.path;
                    match format!("{}", quote!(#path)).as_str() {
                        "crate" => {
                            if let syn::Lit::Str(l) = nv.lit {
                                let span = l.span();
                                let path_token = l.value().parse::<TokenStream>().unwrap();
                                let path_token = quote_spanned!(span=> #path_token);
                                crayfish_path = Some(syn::parse2::<Path>(path_token).unwrap());
                            } else {
                                bad_attr(nv.lit)?
                            }
                        }
                        _  => bad_attr(nv.path)?
                    }
                }
                _ => bad_attr(arg)?,
            }
        }

        Ok(Attributes { crayfish_path })
    }
}
