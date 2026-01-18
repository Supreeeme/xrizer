use proc_macro::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse::Parse, parse_quote, punctuated::Punctuated, ItemStruct, Token};

#[proc_macro_derive(InterfaceImpl, attributes(interface, versions))]
pub fn derive_interface_impl(tokens: TokenStream) -> TokenStream {
    let s = syn::parse_macro_input!(tokens as ItemStruct);
    let name = s.ident;
    let generics_with_bounds = s.generics;
    let generics_idents: Punctuated<&syn::Ident, Token![,]> = generics_with_bounds
        .params
        .iter()
        .map(|p| {
            let syn::GenericParam::Type(p) = p else {
                panic!("expected type generic");
            };
            &p.ident
        })
        .collect();

    let versions_attr = &s
        .attrs
        .iter()
        .find(|a| a.style == syn::AttrStyle::Outer && a.meta.path().is_ident("versions"))
        .expect("missing versions attribute")
        .meta;
    let versions = versions_attr
        .require_list()
        .expect("versions attribute should be a list of versions")
        .parse_args_with(Punctuated::<syn::LitInt, Token![,]>::parse_separated_nonempty)
        .expect("parsing versions failed");

    let interface_attr = s
        .attrs
        .into_iter()
        .find(|a| a.style == syn::AttrStyle::Outer && a.meta.path().is_ident("interface"))
        .expect("missing interface attribute")
        .meta;
    let interface = interface_attr
        .require_name_value()
        .expect("parsing interface failed");

    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(interface),
        ..
    }) = &interface.value
    else {
        panic!("expected string literal for interface");
    };
    let interface = interface.value();

    let interface_versions_and_variants: Vec<(syn::Ident, syn::Ident)> = versions
        .iter()
        .map(|v| {
            let num: u8 = v.base10_parse().unwrap();
            (format_ident!("{interface}{num:03}"), format_ident!("V{v}"))
        })
        .collect();

    let wrapped_variants = interface_versions_and_variants.iter().map(|(interface, variant)| {
        quote! { #variant(openvr::VtableWrapper<vr::#interface, super::#name<#generics_idents>>) }
    });
    let interface_versions = interface_versions_and_variants
        .iter()
        .map(|(interface, _)| {
            quote! { vr::#interface::VERSION }
        });

    let get_vtable_arms = interface_versions_and_variants.iter().enumerate().map(|(index, (interface, variant))| {
        quote! {
            x if x == vr::#interface::VERSION => {
                Some(Box::new(|this: &std::sync::Arc<Self>| {
                    let vtable = this.vtables.0[#index]
                        .get_or_init(|| {
                            WrappedVtable::#variant(<Self as openvr::Inherits<vr::#interface>>::new_wrapped(this))
                        });

                    let WrappedVtable::#variant(vtable) = vtable else {
                        unreachable!("vtable currently holding incorrect version {:?} (expected {})", vtable, stringify!(#variant))
                    };
                    &vtable.base as *const vr::#interface as *mut std::ffi::c_void
                }))
            }
            x if x
                .to_string_lossy()
                .strip_prefix("FnTable:")
                .is_some_and(|x| x == vr::#interface::VERSION.to_string_lossy()) =>
            {
                Some(Box::new(<Self as openvr::Inherits<vr::#interface>>::init_fntable))
            }
        }
    });

    let debug_arms = interface_versions_and_variants.iter().map(|(_, variant)| {
        quote! { Self::#variant(_) => f.write_str(stringify!(#variant)) }
    });

    let mod_name = format_ident!("{}_gen", name.to_string().to_lowercase());
    let num_versions = versions.len();

    quote! {
        use #mod_name::Vtables;
        mod #mod_name {
            use openvr as vr;
            use std::sync::OnceLock;
            use super::*;
            use std::ffi::CStr;

            pub struct Vtables #generics_with_bounds ([OnceLock<WrappedVtable <#generics_idents>>; #num_versions]);
            impl #generics_with_bounds Default for Vtables<#generics_idents> {
                fn default() -> Self {
                    Self(std::array::from_fn(|_| OnceLock::new()))
                }
            }
            enum WrappedVtable #generics_with_bounds {
                #(#wrapped_variants),*
            }

            unsafe impl #generics_with_bounds Sync for WrappedVtable<#generics_idents> {}
            unsafe impl #generics_with_bounds Send for WrappedVtable<#generics_idents> {}

            impl #generics_with_bounds std::fmt::Debug for WrappedVtable<#generics_idents> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
                    match self {
                        #(#debug_arms),*
                    }
                }
            }

            impl #generics_with_bounds vr::InterfaceImpl for super::#name<#generics_idents>{
                fn supported_versions() -> &'static [&'static CStr] {
                    &[
                        #(#interface_versions),*
                    ]
                }

                fn get_version(version: &std::ffi::CStr) -> Option<Box<dyn FnOnce(&std::sync::Arc<Self>) -> *mut std::ffi::c_void>> {
                    match version {
                        #(#get_vtable_arms)*
                        _ => None
                    }
                }
            }
        }
    }.into()
}

#[proc_macro_derive(Backends)]
pub fn supported_backends(tokens: TokenStream) -> TokenStream {
    let backends = syn::parse_macro_input!(tokens as syn::ItemEnum);

    let ident = &backends.ident;
    let names: Vec<_> = backends.variants.iter().map(|v| &v.ident).collect();
    let types: Vec<_> = backends
        .variants
        .iter()
        .map(|v| -> syn::Type {
            assert_eq!(v.fields.len(), 1, "Enum variant should have a single field");
            let ty = &v.fields.iter().next().unwrap().ty;
            #[cfg(feature = "test")]
            {
                if v.attrs.iter().any(|a| {
                    a.meta
                        .require_list()
                        .ok()
                        .is_some_and(|l| l.tokens.to_string() == "test")
                }) {
                    return ty.clone();
                }
            }
            parse_quote! { crate::graphics_backends::#ty }
        })
        .collect();
    let apis: Vec<_> = types
        .iter()
        .map(|t| -> syn::TypePath {
            parse_quote!(<#t as crate::graphics_backends::GraphicsBackend>::Api)
        })
        .collect();

    let tokens = quote! {
        const fn _backend_check<T: GraphicsBackend>() {}
        #( const _: () = _backend_check::<#types>(); )*

        macro_rules! __with_any_graphics_impl {
            ($pub:vis, $($types:ty),+) => {
                macros::__with_any_graphics_impl!($pub, self, [#(#names),*], [$($types),+]);
                macros::__with_any_graphics_impl!($pub, &self, [#(#names),*], [$($types),+]);
                macros::__with_any_graphics_impl!($pub, &mut self, [#(#names),*], [$($types),+]);
            }
        }
        pub(crate) use __with_any_graphics_impl;

        impl #ident {
            __with_any_graphics_impl!(pub, #(#types),*);
        }

        /// Generates an enum where each element holds a struct that is generic over all the
        /// supported graphics APIs
        macro_rules! supported_apis_enum {
            ($pub:vis enum $enum:ident: $($struct:tt)+) => {
                #[derive(derive_more::TryInto, derive_more::From)]
                #[try_into(owned, ref, ref_mut)]
                $pub enum $enum {
                    #(#names($($struct)+<#apis>)),*
                }

                impl<G: xr::Graphics> crate::graphics_backends::GraphicsEnum<G> for $enum
                    where $($struct)+<G>: TryFrom<Self>
                {
                    type Inner = $($struct)+<G>;
                }

                impl $enum {
                    crate::graphics_backends::__with_any_graphics_impl!($pub, #(#apis),*);
                }
            }
        }

        /// Generates an enum where each element holds a struct that is generic over all the
        /// supported graphics APIs
        macro_rules! supported_backends_enum {
            ($pub:vis enum $enum:ident: $($struct:tt)+) => {
                #[derive(derive_more::TryInto, derive_more::From)]
                #[try_into(owned, ref)]
                #[allow(clippy::large_enum_variant)]
                $pub enum $enum {
                    #(#names($($struct)+<#types>)),*
                }

                impl<B> crate::graphics_backends::GraphicsEnum<B> for $enum
                    where B: crate::graphics_backends::GraphicsBackend,
                    $($struct)+<B>: TryFrom<Self>
                {
                    type Inner = $($struct)+<B>;
                }

                impl $enum {
                    crate::graphics_backends::__with_any_graphics_impl!($pub, #(#types),*);
                }
            }
        }

    };
    tokens.into()
}

struct GraphicsImpl {
    vis: syn::Visibility,
    recv: syn::Receiver,
    names: Punctuated<syn::Ident, Token![,]>,
    types: Punctuated<syn::TypePath, Token![,]>,
}

impl Parse for GraphicsImpl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let vis = input.parse()?;
        input.parse::<Token![,]>()?;
        let recv = input.parse()?;
        input.parse::<Token![,]>()?;

        let names;
        syn::bracketed!(names in input);
        let names = Punctuated::parse_separated_nonempty(&names)?;
        input.parse::<Token![,]>()?;

        let types;
        syn::bracketed!(types in input);
        let types = Punctuated::parse_separated_nonempty(&types)?;

        Ok(Self {
            vis,
            recv,
            names,
            types,
        })
    }
}
#[proc_macro]
pub fn __with_any_graphics_impl(tokens: TokenStream) -> TokenStream {
    let GraphicsImpl {
        vis,
        recv,
        names,
        types,
    } = syn::parse_macro_input!(tokens as GraphicsImpl);

    let (trait_name, fn_name) = if recv.reference.is_some() {
        if recv.mutability.is_some() {
            ("WithAnyGraphicsMut", "with_any_graphics_mut")
        } else {
            ("WithAnyGraphics", "with_any_graphics")
        }
    } else {
        ("WithAnyGraphicsOwned", "with_any_graphics_owned")
    };
    let trait_name = syn::Ident::new(trait_name, Span::call_site().into());
    let fn_name = syn::Ident::new(fn_name, Span::call_site().into());
    let names = names.into_iter();
    let types = types.into_iter().collect::<Vec<_>>();

    quote! {
        #vis fn #fn_name<Action>(
            #recv,
            args: <Action as crate::graphics_backends::WithAnyGraphicsParams>::Args
        ) -> <Action as crate::graphics_backends::WithAnyGraphicsParams>::Ret
            where Action: #(crate::graphics_backends::#trait_name<#types, GraphicsEnum = Self>)+*
        {
            match self {
                #( Self::#names(s) => {
                    <Action as crate::graphics_backends::#trait_name<#types>>::with_any_graphics(s, args)
                } )*
            }
        }
    }.into()
}

/// When attached to a generic function, turns it into a struct that can be passed into the
/// `with_any_graphics` method of an enum generated via `supported_apis_enum` or `supported_backends_enum`.
#[proc_macro_attribute]
pub fn any_graphics(attrs: TokenStream, tokens: TokenStream) -> TokenStream {
    let gfx_enum = syn::parse_macro_input!(attrs as syn::TypePath);
    let mut func = syn::parse_macro_input!(tokens as syn::ItemFn);
    let mut args = func.sig.inputs.into_iter();
    let syn::FnArg::Typed(gfx) = args.next().expect("Missing enum argument") else {
        panic!("Arguments should all be idents");
    };
    let gfx_name = gfx.pat;
    let gfx_ty = *gfx.ty;
    let wag_trait = if let syn::Type::Reference(t) = &gfx_ty {
        if t.mutability.is_some() {
            quote!(crate::graphics_backends::WithAnyGraphicsMut)
        } else {
            quote!(crate::graphics_backends::WithAnyGraphics)
        }
    } else {
        quote!(crate::graphics_backends::WithAnyGraphicsOwned)
    };

    // If one of the arguments to the function is a reference, we will need to add lifetime
    // paramters to the generated struct. Parse through passed in arguments including generic
    // paramaters to find a reference.
    let mut lifetime: Option<syn::Lifetime> = None;
    let (tuple_arg_names, tuple_arg_types) =
        args.fold((Vec::new(), Vec::new()), |(mut names, mut args), arg| {
            let syn::FnArg::Typed(arg) = arg else {
                unreachable!();
            };
            names.push(*arg.pat);
            let mut ty = *arg.ty;
            let mut set_lifetime = |lt: &mut syn::TypeReference| {
                lt.lifetime = Some(
                    lifetime
                        .get_or_insert_with(|| parse_quote! { 'args })
                        .clone(),
                );
            };
            match ty {
                syn::Type::Reference(mut rty) => {
                    set_lifetime(&mut rty);
                    ty = rty.into();
                }
                // XXX: This only handles one level of generics.
                syn::Type::Path(ref mut p) => {
                    for segment in &mut p.path.segments {
                        let syn::PathArguments::AngleBracketed(args) = &mut segment.arguments
                        else {
                            continue;
                        };
                        for arg in &mut args.args {
                            if let syn::GenericArgument::Type(syn::Type::Reference(rty)) =
                                arg
                            {
                                set_lifetime(rty);
                            }
                        }
                    }
                }
                other => ty = other,
            }

            args.push(ty);
            (names, args)
        });

    let params = &mut func.sig.generics.params;
    assert_eq!(params.len(), 1, "There should only be one generic paramter");
    let first_param = params.first().unwrap();
    let syn::GenericParam::Type(first_param) = first_param else {
        panic!("The generic parameter should be a type paramater");
    };
    let first_param_ident = first_param.ident.clone();

    let mut phantom_data = None;
    let mut fn_struct_generics = None;
    if let Some(lifetime) = lifetime {
        phantom_data = Some(quote! { (std::marker::PhantomData<& #lifetime ()>) });
        fn_struct_generics = Some(quote! { <#lifetime> });
        params.insert(0, syn::LifetimeParam::new(lifetime).into());
    }

    let ret = func.sig.output;
    let ret_ty = match ret {
        syn::ReturnType::Default => parse_quote!(()),
        syn::ReturnType::Type(_, ref ty) => *ty.clone(),
    };
    let fn_name = func.sig.ident;
    let fn_content = func.block;
    let mut where_clause = func.sig.generics.where_clause.unwrap_or(syn::WhereClause {
        where_token: Default::default(),
        predicates: Default::default(),
    });
    let gfx_ty_no_ref = match gfx_ty.clone() {
        syn::Type::Reference(ty) => *ty.elem,
        other => other,
    };
    where_clause
        .predicates
        .push(parse_quote!(#gfx_ty_no_ref: TryFrom<#gfx_enum>));

    quote! {
        #[allow(non_camel_case_types)]
        #[derive(Default)]
        struct #fn_name #fn_struct_generics #phantom_data;

        impl #fn_struct_generics crate::graphics_backends::WithAnyGraphicsParams for #fn_name #fn_struct_generics {
                type Args = (#(#tuple_arg_types),*);
                type Ret = #ret_ty;
        }

        impl<#params> #wag_trait <#first_param_ident> for #fn_name #fn_struct_generics
            #where_clause
        {
            type GraphicsEnum = #gfx_enum;
            fn with_any_graphics(#gfx_name: #gfx_ty, (#(#tuple_arg_names),*): (#(#tuple_arg_types),*))
                 #ret
                 #fn_content
        }
    }
    .into()
}
