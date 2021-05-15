use proc_macro2::TokenStream;
use quote::quote;
use syn::GenericParam;
use syn::Item;

pub(crate) enum RSendImpl {
    Squashed,
    Squashable,
    DefaultImpl,
}

pub(crate) fn impl_remote_send(arg_type: RSendImpl, item: Item) -> TokenStream {
    // let
    // TODO: support re-export crayfish
    let crayfish_path: TokenStream = "::crayfish".parse().unwrap();
    let remote_send_trait: TokenStream = quote!(#crayfish_path::args::RemoteSend);

    let serde_path = format!("{}::serde", crayfish_path);

    let out_item = quote! {
        #[derive(#crayfish_path::Serialize, #crayfish_path::Deserialize)]
        #[serde(crate = #serde_path )]
        #item
    };

    let (name, generics) = match item {
        Item::Struct(s) => {
            let syn::ItemStruct {
                ident, generics, ..
            } = s;
            (ident, generics)
        }
        Item::Enum(s) => {
            let syn::ItemEnum {
                ident, generics, ..
            } = s;
            (ident, generics)
        }
        Item::Union(s) => {
            let syn::ItemUnion {
                ident, generics, ..
            } = s;
            (ident, generics)
        }
        _ => {
            return quote! {
                compile_error!{"attributes only applys to struct, enum."}
            }
        }
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // attach each ty param the Bound of RemoteSend
    let ty_generices_with_bound: Vec<_> = generics
        .params
        .iter()
        .filter_map(|gp| {
            match gp {
                GenericParam::Type(tp) => Some(tp.ident.clone()),
                _ => None,
            }
            .map(|id| quote!( #id: #remote_send_trait ))
        })
        .collect();
    let where_clause = match where_clause {
        Some(w) => {
            // remove the last comma in where clause
            let predicates = w.predicates.iter();
            Some(quote!(where #(#predicates),* , #(#ty_generices_with_bound), *))
        },
        None => {
            if ty_generices_with_bound.is_empty() {
                None
            } else {
                Some(quote!(where #(#ty_generices_with_bound), *))
            }
        }
    };

    let rsend_impl = quote! {
        #[doc(hidden)]
        #[allow(unused_qualifications, unused_attributes)]
        #[automatically_derived]
        impl #impl_generics #remote_send_trait for #name #ty_generics
            #where_clause
        {
            type Output = ();
            fn is_squashable() -> ::std::primitive::bool {
                false
            }
            fn fold(&self, _acc: &mut Self::Output){
                unreachable!()
            }
            fn extract(_out: &mut Self::Output) -> ::std::option::Option<Self>
            where
                Self: Sized
            {
                unreachable!()
            }
            fn reorder(&self, _other: &Self) -> ::std::cmp::Ordering{
                unreachable!()
            }
        }
    };

    // register for suqashable
    match arg_type {
        RSendImpl::Squashable => {
            if !generics.params.is_empty() {
                return quote! {
                    compile_error!{"current version doesn't support generics for squashable type."}
                };
            }
            return quote! {
                #out_item
                #[doc(hidden)]
                #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
                const _: () = {
                    use #crayfish_path::inventory as inventory;
                    inventory::submit! {
                        #crayfish_path::runtime_meta::SquashHelperMeta::new::<#name>()
                    };
                };
            };
        }
        _ => (),
    }

    quote! {
        #out_item
        #rsend_impl
    }
}
