use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// 为实现了 Deserialize 的类型自动实现 ParseValue trait
///
/// # 示例
/// ```ignore
/// use serde::Deserialize;
/// use rustx::kv::parser::ParseValue;
///
/// #[derive(Debug, Deserialize, ParseValue)]
/// struct User {
///     name: String,
///     age: i32,
/// }
/// ```
#[proc_macro_derive(ParseValue)]
pub fn parse_value_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    // 添加泛型参数支持（如果有）
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics crate::kv::parser::ParseValue for #struct_name #ty_generics #where_clause {
            fn parse_value(s: &str) -> Result<Self, crate::kv::parser::ParserError> {
                ::serde_json::from_str(s).map_err(|e| {
                    crate::kv::parser::ParserError::ParseFailed(
                        format!("failed to parse {}: {}", stringify!(#struct_name), e)
                    )
                })
            }
        }
    };

    TokenStream::from(expanded)
}
