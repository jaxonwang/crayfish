use proc_macro::TokenStream;

#[proc_macro]
pub fn profiling_start_internal(items: TokenStream) -> TokenStream {
    inner::_profiling_start(items, true)
}

#[proc_macro]
pub fn profiling_stop_internal(items: TokenStream) -> TokenStream {
    inner::_profiling_stop(items, true)
}

#[proc_macro]
pub fn profiling_start(items: TokenStream) -> TokenStream {
    inner::_profiling_start(items, false)
}

#[proc_macro]
pub fn profiling_stop(items: TokenStream) -> TokenStream {
    inner::_profiling_stop(items, false)
}

#[cfg(feature = "enabled")]
mod inner {
    use super::*;
    use quote::quote;
    use syn::parse_macro_input;

    fn with_prefix(ident: &str) -> String {
        static UGLY_PREFIX: &str = "___crayfish_trace_macro_prefix_";
        UGLY_PREFIX.to_owned() + ident
    }

    fn string_to_ident(ident: &str) -> syn::Ident {
        syn::Ident::new(ident, proc_macro2::Span::call_site())
    }

    fn span_var_name() -> syn::Ident {
        string_to_ident(&with_prefix("span_marker"))
    }

    fn static_span_var_name(span_name: &str) -> syn::Ident {
        use std::hash::Hash;
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::default();
        span_name.hash(&mut hasher);
        let hashed = hasher.finish();
        let mut static_span_name = with_prefix(&format!("{}_STAIC_HERE", hashed));
        static_span_name.make_ascii_uppercase();
        string_to_ident(&static_span_name)
    }

    fn start_var_name() -> syn::Ident {
        string_to_ident(&with_prefix("start_instant"))
    }

    fn crayfish_path(is_internal: bool) -> syn::Path {
        let crayfish_path = if is_internal { "::crate" } else { "::crayfish" };
        syn::parse_str::<syn::Path>(crayfish_path).unwrap()
    }

    pub fn _profiling_start(items: TokenStream, is_internal: bool) -> TokenStream {
        let crayfish_path = crayfish_path(is_internal);
        let span_name;
        if items.is_empty() {
            span_name = syn::parse_str::<syn::LitStr>("no_name").unwrap();
        } else {
            span_name = parse_macro_input!(items as syn::LitStr);
        }
        let span_var_name = span_var_name();
        let static_span_var_name = static_span_var_name(&span_name.value());
        let start_var_name = start_var_name();
        let ts = quote! {

        let #span_var_name: &#crayfish_path::trace::Span =
        {
            use #crayfish_path::trace::{Span, register_span};
            use #crayfish_path::re_export::once_cell::sync::Lazy;
            static #static_span_var_name : Lazy<Span>= Lazy::new(||{
                let s = Span::new(#span_name, file!(), line!());
                register_span(&s);
                s
            });
            &#static_span_var_name
        };
        let #start_var_name = #crayfish_path::trace::timer_now();

        };
        // eprintln!("{}", ts);
        ts.into()
    }

    pub fn _profiling_stop(_item: TokenStream, is_internal: bool) -> TokenStream {
        let crayfish_path = crayfish_path(is_internal);
        let span_var_name = span_var_name();
        let start_var_name = start_var_name();
        let ts = quote! {
            {
                let stop = #crayfish_path::trace::timer_now();
                #crayfish_path::trace::profile_span_submit(#span_var_name, stop - #start_var_name);
            }
        };
        // eprintln!("{}", ts);
        ts.into()
    }
}

#[cfg(not(feature = "enabled"))]
mod inner {
    use super::*;
    pub fn _profiling_stop(_item: TokenStream, _is_internal: bool) -> TokenStream {
        TokenStream::default()
    }

    pub fn _profiling_start(_items: TokenStream, _is_internal: bool) -> TokenStream {
        TokenStream::default()
    }
}
