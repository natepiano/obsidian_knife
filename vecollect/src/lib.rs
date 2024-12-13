//! adds Deref, DerefMut, IntoIterator, FromIterator traits to a collection
//! that is of the form
//! ```
//!
//! pub struct TThings<T> {
//!     pub things: Vec<T>,
//! }
//! ```
//! makes it convenient to use this common container/contained pattern without having
//! to retype all of this boilerplate
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::Token;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident, Type};

struct CollectionArgs {
    field: String,
}

impl Parse for CollectionArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let vars = Punctuated::<syn::MetaNameValue, Token![,]>::parse_terminated(input)?;
        let mut field = None;

        for var in vars {
            if var.path.is_ident("field") {
                if let syn::Expr::Lit(lit) = var.value {
                    if let syn::Lit::Str(s) = lit.lit {
                        field = Some(s.value());
                    }
                }
            }
        }

        Ok(CollectionArgs {
            field: field.ok_or_else(|| {
                syn::Error::new_spanned(input.to_string(), "field argument is required")
            })?,
        })
    }
}

#[proc_macro_attribute]
pub fn collection(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as CollectionArgs);
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let field_name = Ident::new(&args.field, proc_macro2::Span::call_site());

    // Extract the type from the Vec<T> field
    let inner_type = if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields) = &data_struct.fields {
            fields
                .named
                .iter()
                .find(|f| f.ident.as_ref().map_or(false, |i| i == &field_name))
                .and_then(|field| {
                    if let Type::Path(type_path) = &field.ty {
                        type_path
                            .path
                            .segments
                            .iter()
                            .find(|seg| seg.ident == "Vec")
                            .and_then(|seg| {
                                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                                    args.args.first()
                                } else {
                                    None
                                }
                            })
                    } else {
                        None
                    }
                })
        } else {
            None
        }
    } else {
        None
    }
    .expect("Field must be Vec<T>");

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        #input

        impl #impl_generics std::ops::Deref for #name #ty_generics #where_clause {
            type Target = Vec<#inner_type>;

            fn deref(&self) -> &Self::Target {
                &self.#field_name
            }
        }

        impl #impl_generics std::ops::DerefMut for #name #ty_generics #where_clause {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.#field_name
            }
        }

        impl #impl_generics FromIterator<#inner_type> for #name #ty_generics #where_clause {
            fn from_iter<I: IntoIterator<Item = #inner_type>>(iter: I) -> Self {
                Self {
                    #field_name: iter.into_iter().collect(),
                }
            }
        }

        impl #impl_generics IntoIterator for #name #ty_generics #where_clause {
            type Item = #inner_type;
            type IntoIter = std::vec::IntoIter<#inner_type>;

            fn into_iter(self) -> Self::IntoIter {
                self.#field_name.into_iter()
            }
        }

        impl<'a> IntoIterator for &'a #name #ty_generics #where_clause {
            type Item = &'a #inner_type;
            type IntoIter = std::slice::Iter<'a, #inner_type>;

            fn into_iter(self) -> Self::IntoIter {
                self.#field_name.iter()
            }
        }

        impl<'a> IntoIterator for &'a mut #name #ty_generics #where_clause {
            type Item = &'a mut #inner_type;
            type IntoIter = std::slice::IterMut<'a, #inner_type>;

            fn into_iter(self) -> Self::IntoIter {
                self.#field_name.iter_mut()
            }
        }
    };

    TokenStream::from(expanded)
}
