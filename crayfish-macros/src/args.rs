use quote::quote;
use syn::Item;
use proc_macro2::TokenStream;

pub enum RSendImpl{
    Squashed,
    Squashable,
    DefaultImpl,
}


pub fn impl_remote_send(arg_type: RSendImpl, item: Item) -> TokenStream {

    // let 
    // TODO: support re-export crayfish
    let crayfish_path: TokenStream = "::crayfish".parse().unwrap();
    let serde_path = format!("{}::serde", crayfish_path );

    let out_item = quote!{
        #[derive(#crayfish_path::Serialize, #crayfish_path::Deserialize)]
        #[serde(crate = #serde_path )]
        #item
    };
    match arg_type{
        RSendImpl::Squashable => {
            return TokenStream::from(out_item);
        }
        _ => ()
    }

    let (name, generics) = match item{
        Item::Struct(s) =>
        {
            let syn::ItemStruct{ident, generics, ..} = s;
            (ident, generics)
        },
        Item::Enum(s) =>
        {
            let syn::ItemEnum{ident, generics, ..} = s;
            (ident, generics)
        },
        Item::Union(s) =>
        {
            let syn::ItemUnion{ident, generics, ..} = s;
            (ident, generics)
        },
        _ => 
            return quote!{
                compile_error!{"attributes only applys to struct, enum."}
            }
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let rsend_impl = quote! {
        #[doc(hidden)]
        #[allow(unused_qualifications, unused_attributes)]
        #[automatically_derived]
        impl #impl_generics #crayfish_path::args::RemoteSend for #name #ty_generics #where_clause {
            type Output = ();
            fn is_squashable() -> ::std::primitive::bool {
                false
            }
            fn fold(&self, _acc: &mut Self::Output){
                panic!()
            }
            fn extract(_out: &mut Self::Output) -> ::std::option::Option<Self>
            where
                Self: Sized
            {
                panic!()
            }
            fn reorder(&self, _other: &Self) -> ::std::cmp::Ordering{
                panic!()
            }
        }
    };
    quote!{
        #out_item
        #rsend_impl
    }
}
