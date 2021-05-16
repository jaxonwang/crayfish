use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use quote::ToTokens;
use syn::AttributeArgs;
use syn::Error;
use syn::Meta;
use syn::NestedMeta;
use syn::Path;
use syn::Type;

pub struct Attributes {
    pub crayfish_path: Option<Path>,
    pub ret_type: Option<Type>,
}

const BAD_ATTR: &str = "bad attribute";

fn bad_attr(tokens: impl ToTokens) -> syn::Result<()> {
    Err(Error::new_spanned(tokens, BAD_ATTR))
}

fn str_literal_parse<T: syn::parse::Parse>(lit: &syn::Lit) -> syn::Result<T> {
    if let syn::Lit::Str(l) = lit {
        let span = l.span();
        let token = l.value().parse::<TokenStream>().unwrap();
        let token = quote_spanned!(span=> #token);
        syn::parse2::<T>(token)
    } else {
        Err(Error::new_spanned(lit, BAD_ATTR))
    }
}

impl Attributes {
    pub fn new(args: AttributeArgs) -> syn::Result<Self> {
        let mut crayfish_path: Option<Path> = None;
        let mut ret_type: Option<Type> = None;

        for arg in args {
            match arg {
                NestedMeta::Meta(Meta::NameValue(nv)) => {
                    let path = &nv.path;
                    match format!("{}", quote!(#path)).as_str() {
                        "crate" => {
                            crayfish_path = Some(str_literal_parse::<Path>(&nv.lit)?);
                        }
                        "ret" => {
                            ret_type = Some(str_literal_parse::<Type>(&nv.lit)?);
                        }
                        _ => bad_attr(nv.path)?,
                    }
                }
                _ => bad_attr(arg)?,
            }
        }

        Ok(Attributes {
            crayfish_path,
            ret_type,
        })
    }
}
