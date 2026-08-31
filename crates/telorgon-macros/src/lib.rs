//! Procedural component-field classification for Telorgon's composition API.

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Attribute, Fields, ItemStruct, Meta, Path, Token, parse_macro_input};

enum InputMode {
    Compare,
    Always,
    CompareWith(Path),
}

enum FieldKind {
    Input(InputMode),
    State,
}

/// Classifies a persistent component's fields as parent-owned inputs or locally persistent state.
#[proc_macro_attribute]
pub fn component(arguments: TokenStream, item: TokenStream) -> TokenStream {
    let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
    let arguments = match parser.parse(arguments) {
        Ok(arguments) => arguments,
        Err(error) => return error.into_compile_error().into(),
    };
    let mut crate_path = None;
    let mut derive_default = true;
    for argument in arguments {
        match argument {
            Meta::Path(path) if path.is_ident("no_default") => {
                derive_default = false;
            }
            Meta::NameValue(value) if value.path.is_ident("crate_path") => {
                if crate_path.is_some() {
                    return syn::Error::new_spanned(value, "duplicate crate_path argument")
                        .into_compile_error()
                        .into();
                }
                let syn::Expr::Path(path) = value.value else {
                    return syn::Error::new_spanned(value.value, "crate_path must be a Rust path")
                        .into_compile_error()
                        .into();
                };
                crate_path = Some(path.path);
            }
            other => {
                return syn::Error::new_spanned(other, "expected no_default or crate_path = path")
                    .into_compile_error()
                    .into();
            }
        }
    }

    let fields_trait = if let Some(path) = crate_path {
        quote!(#path::__private::ComponentFields)
    } else {
        match crate_name("telorgon") {
            Ok(FoundCrate::Itself) => quote!(::telorgon::compose::__private::ComponentFields),
            Ok(FoundCrate::Name(name)) => {
                let name = format_ident!("{name}");
                quote!(::#name::compose::__private::ComponentFields)
            }
            Err(error) => {
                return syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!("#[component] must be used through a telorgon dependency: {error}"),
                )
                .into_compile_error()
                .into();
            }
        }
    };

    let mut item = parse_macro_input!(item as ItemStruct);
    let Fields::Named(fields) = &mut item.fields else {
        return syn::Error::new_spanned(&item, "#[component] requires a struct with named fields")
            .into_compile_error()
            .into();
    };

    let mut classified = Vec::with_capacity(fields.named.len());
    for field in &mut fields.named {
        let mut kind = None;
        let mut retained = Vec::with_capacity(field.attrs.len());
        for attribute in std::mem::take(&mut field.attrs) {
            if attribute.path().is_ident("input") {
                if kind.is_some() {
                    return syn::Error::new_spanned(
                        attribute,
                        "a component field must have exactly one of #[input] or #[state]",
                    )
                    .into_compile_error()
                    .into();
                }
                match parse_input_mode(&attribute) {
                    Ok(mode) => kind = Some(FieldKind::Input(mode)),
                    Err(error) => return error.into_compile_error().into(),
                }
            } else if attribute.path().is_ident("state") {
                if kind.is_some() {
                    return syn::Error::new_spanned(
                        attribute,
                        "a component field must have exactly one of #[input] or #[state]",
                    )
                    .into_compile_error()
                    .into();
                }
                if !matches!(attribute.meta, Meta::Path(_)) {
                    return syn::Error::new_spanned(
                        attribute,
                        "#[state] does not accept arguments",
                    )
                    .into_compile_error()
                    .into();
                }
                kind = Some(FieldKind::State);
            } else {
                retained.push(attribute);
            }
        }
        field.attrs = retained;
        let Some(kind) = kind else {
            return syn::Error::new_spanned(
                field,
                "every #[component] field must be marked #[input] or #[state]",
            )
            .into_compile_error()
            .into();
        };
        classified.push(kind);
    }

    if derive_default && !has_default_derive(&item.attrs) {
        item.attrs.push(syn::parse_quote!(#[derive(Default)]));
    }

    let fields: Vec<_> = fields.named.iter().cloned().collect();
    let name = &item.ident;
    let generics = &item.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let identifiers: Vec<_> = fields
        .iter()
        .map(|field| field.ident.as_ref().expect("named field"))
        .collect();
    let incoming_identifiers: Vec<_> = identifiers
        .iter()
        .map(|identifier| format_ident!("__telorgon_incoming_{identifier}"))
        .collect();

    let destructure = identifiers
        .iter()
        .zip(incoming_identifiers.iter())
        .map(|(field, incoming)| quote!(#field: #incoming));

    let input_fields: Vec<_> = fields
        .iter()
        .zip(classified.iter())
        .filter_map(|(field, kind)| match kind {
            FieldKind::Input(mode) => Some((field, mode)),
            FieldKind::State => None,
        })
        .collect();

    let snapshot_types = input_fields.iter().map(|(field, _)| &field.ty);
    let snapshot_values = input_fields.iter().map(|(field, _)| {
        let identifier = field.ident.as_ref().expect("named field");
        quote!(self.#identifier.clone())
    });
    let snapshot_bindings: Vec<_> = input_fields
        .iter()
        .map(|(field, _)| {
            let identifier = field.ident.as_ref().expect("named field");
            format_ident!("__telorgon_snapshot_{identifier}")
        })
        .collect();

    let updates = fields
        .iter()
        .zip(classified.iter())
        .zip(incoming_identifiers.iter())
        .filter_map(|((field, kind), incoming)| {
            let identifier = field.ident.as_ref().expect("named field");
            let FieldKind::Input(mode) = kind else {
                return None;
            };
            Some(match mode {
                InputMode::Compare => quote! {
                    if self.#identifier != #incoming {
                        self.#identifier = #incoming;
                        __telorgon_changed = true;
                    }
                },
                InputMode::Always => quote! {
                    self.#identifier = #incoming;
                    __telorgon_changed = true;
                },
                InputMode::CompareWith(path) => quote! {
                    if !#path(&self.#identifier, &#incoming) {
                        self.#identifier = #incoming;
                        __telorgon_changed = true;
                    }
                },
            })
        });

    let restores =
        input_fields
            .iter()
            .zip(snapshot_bindings.iter())
            .map(|((field, mode), snapshot)| {
                let identifier = field.ident.as_ref().expect("named field");
                match mode {
                    InputMode::Compare => quote! {
                        if self.#identifier != #snapshot {
                            __telorgon_mutated = true;
                        }
                        self.#identifier = #snapshot;
                    },
                    InputMode::Always => quote! {
                        self.#identifier = #snapshot;
                    },
                    InputMode::CompareWith(path) => quote! {
                        if !#path(&self.#identifier, &#snapshot) {
                            __telorgon_mutated = true;
                        }
                        self.#identifier = #snapshot;
                    },
                }
            });

    quote! {
        #item

        impl #impl_generics #fields_trait
            for #name #ty_generics #where_clause
        {
            type InputSnapshot = (#(#snapshot_types,)*);

            fn update_inputs(&mut self, incoming: Self) -> bool {
                let Self { #(#destructure,)* } = incoming;
                let mut __telorgon_changed = false;
                #(#updates)*
                __telorgon_changed
            }

            fn capture_inputs(&self) -> Self::InputSnapshot {
                (#(#snapshot_values,)*)
            }

            fn restore_inputs(&mut self, snapshot: Self::InputSnapshot) -> bool {
                let (#(#snapshot_bindings,)*) = snapshot;
                let mut __telorgon_mutated = false;
                #(#restores)*
                __telorgon_mutated
            }
        }
    }
    .into()
}

fn has_default_derive(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("derive") {
            return false;
        }
        let mut found = false;
        let _ = attribute.parse_nested_meta(|meta| {
            if meta
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "Default")
            {
                found = true;
            }
            Ok(())
        });
        found
    })
}

fn parse_input_mode(attribute: &Attribute) -> syn::Result<InputMode> {
    match &attribute.meta {
        Meta::Path(_) => Ok(InputMode::Compare),
        Meta::NameValue(_) => Err(syn::Error::new_spanned(
            attribute,
            "use #[input], #[input(always)], or #[input(compare_with = path)]",
        )),
        Meta::List(list) => {
            let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
            let arguments = parser.parse2(list.tokens.clone())?;
            if arguments.len() != 1 {
                return Err(syn::Error::new_spanned(
                    list,
                    "#[input] accepts exactly one mode",
                ));
            }
            match arguments.first().expect("one argument") {
                Meta::Path(path) if path.is_ident("always") => Ok(InputMode::Always),
                Meta::NameValue(value) if value.path.is_ident("compare_with") => {
                    let syn::Expr::Path(path) = &value.value else {
                        return Err(syn::Error::new_spanned(
                            &value.value,
                            "compare_with must be a function path",
                        ));
                    };
                    Ok(InputMode::CompareWith(path.path.clone()))
                }
                other => Err(syn::Error::new_spanned(
                    other,
                    "expected always or compare_with = path",
                )),
            }
        }
    }
}
