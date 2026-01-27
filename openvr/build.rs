use bindgen::callbacks::ParseCallbacks;

use prettyplease::unparse;
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident};
use regex::Regex;
use std::collections::{HashMap, hash_map::Entry};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::str::FromStr;
use std::sync::LazyLock;
use syn::parse_quote;

#[allow(unused_macros)]
macro_rules! dbg {
    ($($tokens:tt)*) => {
        for line in format!($($tokens)*).lines() {
            println!("cargo:warning={line}")
        }
    }
}

// Bindgen will generate UB for the Default implementation of structs
// that have a rustified enum member.
// We will instead manually place a derive(Default) on these.
// https://github.com/rust-lang/rust-bindgen/issues/2974
static MANUAL_DERIVE_DEFAULT: &[&str] = &[
    // Have a vr::ETrackingResult member
    "TrackedDevicePose_t",
    "InputPoseActionData_t",
    "Compositor_FrameTiming",
];

#[derive(Debug)]
struct Callbacks;

impl ParseCallbacks for Callbacks {
    fn add_derives(&self, info: &bindgen::callbacks::DeriveInfo<'_>) -> Vec<String> {
        use bindgen::callbacks::TypeKind;
        match (info.kind, info.name) {
            (TypeKind::Struct, x) if MANUAL_DERIVE_DEFAULT.contains(&x) => {
                vec!["Default".to_string()]
            }
            (TypeKind::Enum, "EVREventType") => {
                vec!["derive_more::TryFrom".to_string()]
            }
            _ => Vec::new(),
        }
    }

    // All of the enums have an annoying prefix that is just the name
    // Since Rust enums are properly scoped, we can just strip this prefix
    fn enum_variant_name(
        &self,
        enum_name: Option<&str>,
        original_variant_name: &str,
        _variant_value: bindgen::callbacks::EnumVariantValue,
    ) -> Option<String> {
        let enum_name = enum_name?;
        match original_variant_name.split_once('_') {
            Some(("k", name)) => name.split_once('_').map(|s| s.1.to_string()).or_else(|| {
                if enum_name == "EHiddenAreaMesh" {
                    Some(name.split_once('_')?.1.to_string())
                } else {
                    name.strip_prefix(enum_name).map(str::to_string)
                }
            }),
            Some((_, name)) => Some(name.to_string()),
            None => (enum_name == "ETrackingUniverseOrigin")
                .then(|| {
                    original_variant_name
                        .strip_prefix("TrackingUniverse")
                        .map(str::to_string)
                })
                .flatten(),
        }
    }
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    macro_rules! version {
        ($major:literal, $minor:literal, $build:literal) => {
            (
                concat!("openvr-", $major, ".", $minor, ".", $build, ".h"),
                concat!($major, "_", $minor, "_", $build),
            )
        };
    }

    let header_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("headers");
    let headers = [
        version!(2, 12, 14),
        version!(2, 5, 1),
        version!(2, 0, 10),
        version!(1, 23, 7),
        version!(1, 16, 8),
        version!(1, 14, 15),
        version!(1, 8, 19),
        version!(1, 7, 15),
        version!(1, 4, 18),
        version!(1, 3, 22),
        version!(1, 0, 17),
        version!(1, 0, 11),
        version!(1, 0, 10),
        version!(1, 0, 9),
        version!(1, 0, 7),
        version!(1, 0, 5),
        version!(1, 0, 4),
        version!(1, 0, 3),
        version!(1, 0, 1),
        version!(0, 9, 20),
        version!(0, 9, 12),
    ];
    let mut pruned_headers = headers.map(|(header, version)| {
        (
            header,
            prune_header(header_dir.join(header).to_str().unwrap(), version),
        )
    });

    // mark types as 2.5.1 (arbitrary)
    let clientcore3 = prune_header(
        header_dir.join("ivrclientcore_003.h").to_str().unwrap(),
        "2_5_1",
    );
    pruned_headers[0].1.push_str(&clientcore3);
    let clientcore2 = prune_header(
        header_dir.join("ivrclientcore_002.h").to_str().unwrap(),
        "2_0_10",
    );
    pruned_headers[0].1.push_str(&clientcore2);

    let mut builder = bindgen::builder();
    for (header, content) in pruned_headers {
        builder = builder.header_contents(header, &content);
    }

    // Interfaces to generate bindings for.
    const INTERFACES: &[&str] = &[
        "IVRSystem",
        "IVRCompositor",
        "IVROverlay",
        "IVROverlayView",
        "IVRInput",
        "IVRRenderModels",
        "IVRScreenshots",
        "IVRClientCore",
        "IVRChaperone",
        "IVRApplications",
        "IVRSettings",
    ];

    for interface in INTERFACES {
        builder = builder
            .allowlist_type(format!("vr.*::{interface}"))
            .allowlist_item(format!("vr.*::{interface}_Version"));
    }

    for s in MANUAL_DERIVE_DEFAULT {
        builder = builder.no_default(format!("vr.*::{s}"));
    }
    let bindings = builder
        .clang_args([
            "-x",
            "c++",
            "-fparse-all-comments",
            "-DOPENVR_INTERFACE_INTERNAL",
            "-DGNUC",
        ])
        .enable_cxx_namespaces()
        .allowlist_item("vr.*::k_.*") // constants
        .allowlist_item("vr.*::VR.*")
        .allowlist_item("vr.*::EVRComponentProperty")
        .allowlist_item("vr.*::EKeyboardFlags")
        .allowlist_item("Vk.*")
        .blocklist_function("vr.*::VR_.*")
        .derive_default(true)
        .no_default("vr.*::IVR.*")
        .bitfield_enum("vr.*::EVRSubmitFlags")
        .bitfield_enum("vr.*::EVRComponentProperty")
        .bitfield_enum("vr.*::EKeyboardFlags")
        .rustified_enum(".*")
        .vtable_generation(true)
        .generate_cstr(true)
        .generate_comments(true)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .parse_callbacks(Box::new(Callbacks))
        .generate()?;

    let bindings = process_and_versionify_types(bindings.to_string().parse().unwrap());
    let path = Path::new(&std::env::var("OUT_DIR")?).join("bindings.rs");
    std::fs::write(&path, bindings).expect("Couldn't write bindings");
    Ok(())
}

fn prune_header(header: &str, version: &str) -> String {
    static VR_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new("([^a-zA-Z0-9_])(vr)([^a-zA-Z0-9_]|$)").unwrap());
    static TO_DELETE: &[&str] = &[
        "#define _OPENVR_API",
        "#define _INCLUDE",
        "#define INVALID",
        "#define VR_COMPOSITOR",
    ];

    let version = format!("vr_{version}");
    let reader = BufReader::new(
        File::open(header).unwrap_or_else(|e| panic!("Couldn't open {header}: {e}")),
    );
    let mut out = String::new();
    for line in reader.lines().map(|l| l.unwrap()) {
        // Everything after this line ends up being the C facing API, which is the same across versions and bindgen cannot handle it
        // Later versions have it behind an ifdef, but <=1.0.9 doesn't, so we'll just stop
        // processing once we see this line.
        if line == "#endif // _OPENVR_API" {
            out.push_str(&line);
            break;
        }

        if TO_DELETE.iter().any(|s| line.starts_with(*s)) || line.ends_with("VRHeadsetView();") {
            continue;
        }

        let mut line = VR_REGEX.replace_all(&line, &format!("${{1}}{version}${{3}}"));
        line += "\n";
        out.push_str(&line);
    }

    println!("cargo:rerun-if-changed={header}");
    out
}

fn verify_fields_are_identical<'a, T>(
    ident: &syn::Ident,
    existing: T,
    existing_mod: &syn::Ident,
    new: T,
    new_mod: &syn::Ident,
) where
    T: IntoIterator<Item = &'a syn::Field>,
{
    for (existing_field, new_field) in existing.into_iter().zip(new.into_iter()) {
        if existing_field.ident != new_field.ident {
            let idents = [
                existing_field.ident.as_ref().unwrap().to_string(),
                new_field.ident.as_ref().unwrap().to_string(),
            ];
            assert!(
                idents.contains(&"repeatCount".into()) && idents.contains(&"unused".into()),
                "Non-allowed differently named fields in {ident} (left = {:?} from {existing_mod}, right = {:?} from {new_mod})",
                existing_field.ident,
                new_field.ident
            );
        }

        fn extract_type<'a>(
            ident: &syn::Ident,
            field: &'a syn::Field,
            m: &'a syn::Ident,
        ) -> &'a syn::TypePath {
            fn extract_array_type(ty: &syn::Type) -> &syn::Type {
                match ty {
                    syn::Type::Array(ty) => extract_array_type(&ty.elem),
                    other => other,
                }
            }
            match extract_array_type(&field.ty) {
                syn::Type::Path(ty) => ty,
                syn::Type::Ptr(ty) => match &*ty.elem {
                    syn::Type::Path(ty) => ty,
                    _ => panic!(
                        "wrong pointer type: {:?} (field: {}, mod: {}, item: {ident})",
                        field.ty.to_token_stream().to_string(),
                        field.ident.as_ref().unwrap(),
                        m
                    ),
                },
                _ => panic!(
                    "wrong type: {:?} (field: {}, mod: {}, item: {ident})",
                    field.ty.to_token_stream().to_string(),
                    field.ident.as_ref().unwrap(),
                    m
                ),
            }
        }

        let existing_type = extract_type(ident, existing_field, existing_mod);
        let existing_type = existing_type
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        let new_type = extract_type(ident, new_field, new_mod);
        let new_type = new_type
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());

        static KNOWN_NAME_CHANGES: &[(&str, &str)] = &[
            ("ETextureType", "EGraphicsAPIConvention"),
            ("u32", "EVRButtonId"),
            ("u32", "EVRMouseButton"),
            ("u32", "EVRState"),
        ];

        if existing_type
            .as_ref()
            .zip(new_type.as_ref())
            .is_none_or(|(existing, new)| !KNOWN_NAME_CHANGES.contains(&(existing, new)))
        {
            assert_eq!(
                existing_type,
                new_type,
                concat!(
                    "Differently named final path segments ",
                    "(member {:?}, item {}, existing: {}, new: {})"
                ),
                new_field.ident.as_ref(),
                ident,
                existing_mod,
                new_mod
            );
        }
    }
}

/// Remove "root::vr_(version)" from a path. If the path didn't look like this it remains
/// unchanged.
fn unversion_path(path: &mut syn::Path) {
    if path.segments.len() == 1
        || path
            .segments
            .first()
            .is_none_or(|segment| segment.ident != "root")
    {
        return;
    }

    // strip root
    let iter = std::mem::take(&mut path.segments).into_iter();
    let err = iter.clone();
    let mut segments = iter.skip(1).peekable();

    // types of interest will start with "root::vr_(version)"
    if !segments
        .peek()
        .unwrap_or_else(|| {
            let remaining: TokenStream = parse_quote!(#(#err)::*);
            panic!("path is only root? ( {remaining} )",)
        })
        .ident
        .to_string()
        .starts_with("vr")
    {
        path.segments = segments.collect();
        return;
    }

    // drop vr_(version)
    segments.next();

    // we only care about what's left
    path.segments = segments.collect();
}

fn extract_array_or_ptr_type(array: &mut syn::Type) -> &mut syn::Type {
    match array {
        syn::Type::Array(a) => extract_array_or_ptr_type(&mut a.elem),
        syn::Type::Ptr(p) => extract_array_or_ptr_type(&mut p.elem),
        syn::Type::Reference(r) => extract_array_or_ptr_type(&mut r.elem),
        other => other,
    }
}

fn unversion_type(ty: &mut syn::Type) -> Result<(), String> {
    match extract_array_or_ptr_type(ty) {
        syn::Type::Path(ty) => {
            unversion_path(&mut ty.path);
            Ok(())
        }
        syn::Type::BareFn(ty) => {
            for arg in &mut ty.inputs {
                unversion_type(&mut arg.ty).unwrap();
            }
            if let syn::ReturnType::Type(_, ty) = &mut ty.output {
                unversion_type(ty).unwrap();
            }
            Ok(())
        }
        syn::Type::Array(_) | syn::Type::Ptr(_) | syn::Type::Reference(_) => unreachable!(),
        other => Err(format!("unexpected type: {}", other.to_token_stream())),
    }
}

fn unversion_fields<'a, T>(fields: T)
where
    T: IntoIterator<Item = &'a mut syn::Field>,
{
    for field in fields.into_iter() {
        unversion_type(&mut field.ty).unwrap();
    }
}

fn unversion_impl(impl_item: &mut syn::ItemImpl) {
    unversion_type(&mut impl_item.self_ty).unwrap();
    for item in impl_item.items.iter_mut() {
        match item {
            syn::ImplItem::Fn(item) => {
                let sig = &mut item.sig;
                for input in &mut sig.inputs {
                    if let syn::FnArg::Typed(input) = input {
                        unversion_type(&mut input.ty).unwrap();
                    }
                }

                if let syn::ReturnType::Type(_, ty) = &mut sig.output {
                    unversion_type(ty).unwrap();
                }
            }
            syn::ImplItem::Const(item) => {
                unversion_type(&mut item.ty).unwrap();
                match &mut item.expr {
                    syn::Expr::Path(syn::ExprPath { path, .. }) => {
                        unversion_path(path);
                    }
                    syn::Expr::Call(syn::ExprCall { func, .. }) => {
                        if let syn::Expr::Path(syn::ExprPath { path, .. }) = &mut **func {
                            unversion_path(path);
                        }
                    }
                    _ => {}
                }
            }
            syn::ImplItem::Type(_) => {}
            _ => todo!("impl item: {item:?}"),
        }
    }
}

/// Versions look like
#[derive(PartialEq, Eq, Debug)]
struct HeaderVersion {
    major: u32,
    minor: u32,
    build: u32,
}

impl PartialOrd for HeaderVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.major > other.major {
            Some(std::cmp::Ordering::Greater)
        } else if self.major < other.major {
            Some(std::cmp::Ordering::Less)
        } else if self.minor > other.major {
            Some(std::cmp::Ordering::Greater)
        } else if self.minor < other.minor {
            Some(std::cmp::Ordering::Less)
        } else if self.build > other.build {
            Some(std::cmp::Ordering::Greater)
        } else if self.build < other.build {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Equal)
        }
    }
}

impl FromStr for HeaderVersion {
    type Err = String;
    /// Parses a vr mod (like vr_<major>_<minor>_<build>)
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // skip vr_
        let slice = &s[3..];
        let e_to_str = |s| move |e| format!("{e:?} ({s}, {slice})");

        let mut parts = slice.split_terminator('_');
        let major = parts
            .next()
            .ok_or(format!("missing major version: {slice}"))?;
        let major: u32 = major.parse().map_err(e_to_str(major))?;
        let minor = parts
            .next()
            .ok_or(format!("missing minor version: {slice}"))?;
        let minor: u32 = minor.parse().map_err(e_to_str(minor))?;
        let build = parts
            .next()
            .ok_or(format!("missing build version: {slice}"))?;
        let build: u32 = build.parse().map_err(e_to_str(build))?;
        Ok(HeaderVersion {
            major,
            minor,
            build,
        })
    }
}

/// key is item name, value is (item, mod ident)
type UnversionedItems = HashMap<String, (syn::Item, syn::Ident)>;

/// key is interface name i.e. IVRSystem)
type VersionedInterfaces = HashMap<String, Vec<VersionedInterface>>;

struct VersionedInterface {
    version: u32,
    item: syn::ItemStruct,
    vtable: syn::ItemStruct,
    parent_mod: syn::Ident,
}

enum ImplItem {
    // Item that can go in a free impl block
    Free(Box<syn::ImplItem>),
    // Trait implementation block
    TraitImpl(Box<syn::ItemImpl>),
}

#[derive(PartialEq, Eq, Hash)]
struct ImplMapKey {
    /// The type that the corresponding item is being implemented on
    impl_type: String,
    /// The module this item originated from
    vr_mod: String,
}
type ImplMap = HashMap<ImplMapKey, Vec<ImplItem>>;

/// Processes the given version for an item and extracts the versioned struct and vtable from our
/// unversioned interfaces.
fn versionify_interface(
    unversioned: &mut UnversionedItems,
    version_item: syn::ItemConst,
    item_mod: &syn::Ident,
    versioned: &mut VersionedInterfaces,
    incompat: &IncompatibleItems,
) {
    static VERSION_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new("\"([a-zA-Z]+)_([0-9]+)").unwrap());
    // Extract the version and struct name
    let version_str = version_item.to_token_stream().to_string();
    let caps = VERSION_REGEX.captures(&version_str).unwrap_or_else(|| {
        panic!("Couldn't find version number from version const ({version_str})")
    });
    let interface = caps.get(1).unwrap().as_str();
    let version_str = caps.get(2).unwrap().as_str();

    // pull out associated interface and vtable
    // the version always comes after the interfaces so this shoudn't fail
    let Some((syn::Item::Struct(mut versioned_struct), s_mod)) = unversioned.remove(interface)
    else {
        unreachable!();
    };

    let Some((syn::Item::Struct(mut vtable), v_mod)) =
        unversioned.remove(&format!("{interface}__bindgen_vtable"))
    else {
        unreachable!();
    };
    assert_eq!(s_mod, v_mod);

    let mut replace = None;
    // check if we already pulled this version
    let version_num: u32 = version_str.parse().unwrap();
    if let Some(versions) = versioned.get_mut(interface)
        && let Some(old_item) = versions.iter_mut().find(|item| item.version == version_num)
    {
        let old_version: HeaderVersion = old_item.parent_mod.to_string().parse().unwrap();
        let new_version: HeaderVersion = item_mod.to_string().parse().unwrap();

        // only replace it if we have the same version from a newer header -
        // the headers like to add new things without updating versions
        if old_version >= new_version {
            return;
        }

        replace = Some(old_item);
    }

    // change names
    let versioned_name = format!("{interface}{version_str}");
    vtable.ident = format_ident!("{versioned_name}__bindgen_vtable");
    versioned_struct.ident = syn::Ident::new(&versioned_name, versioned_struct.ident.span());

    // change fields
    assert_eq!(versioned_struct.fields.len(), 1);
    let vtable_field = versioned_struct.fields.iter_mut().next().unwrap();
    let syn::Type::Ptr(ty) = &mut vtable_field.ty else {
        panic!("vtable field was not pointer");
    };
    let vtable_ident = &vtable.ident;
    *ty.elem = parse_quote!(#vtable_ident);

    for field in vtable.fields.iter_mut() {
        let syn::Type::BareFn(f) = &mut field.ty else {
            unreachable!();
        };

        let mut args = f.inputs.iter_mut();

        // versionify this
        let this = args.next().unwrap();
        assert_eq!(this.name.as_ref().unwrap().0, "this");
        let syn::Type::Ptr(this) = &mut this.ty else {
            unreachable!();
        };
        let versioned_ident = &versioned_struct.ident;
        *this.elem = parse_quote!(#versioned_ident);

        for arg in args {
            let syn::Type::Path(path) = extract_array_or_ptr_type(&mut arg.ty) else {
                unreachable!()
            };

            if path
                .path
                .get_ident()
                .is_some_and(|i| incompat.contains_key(&i.to_string()))
            {
                path.path
                    .segments
                    .insert(0, syn::PathSegment::from(item_mod.clone()));
            }
        }
    }

    let ret = VersionedInterface {
        version: version_num,
        item: versioned_struct,
        vtable,
        parent_mod: item_mod.clone(),
    };
    if let Some(replace) = replace {
        *replace = ret;
    } else {
        versioned
            .entry(interface.to_string())
            .or_default()
            .push(ret);
    }
}

type IncompatibleItems = HashMap<String, syn::Item>;
fn process_vr_namespace_content(
    unversioned: &mut UnversionedItems,
    versioned: &mut VersionedInterfaces,
    impl_map: &mut ImplMap,
    vr_mod: syn::ItemMod,
) {
    // Items that clash with types from other version
    let mut incompatible_items = IncompatibleItems::new();
    let span = vr_mod.ident.span();
    let reversion_type = |incompatible_items: &IncompatibleItems, ty: &mut syn::Type| {
        fn inner(
            span: proc_macro2::Span,
            incompatible_items: &IncompatibleItems,
            ty: &mut syn::Type,
        ) {
            static PRIMITIVES: &[&str] = &["u32", "i32", "f32", "u64", "f64", "bool", "c_char"];
            match extract_array_or_ptr_type(ty) {
                syn::Type::Path(type_path) => {
                    let type_name = type_path.path.segments.last().unwrap().ident.to_string();
                    if !incompatible_items.contains_key(&type_name)
                        && !PRIMITIVES.contains(&type_name.as_str())
                    {
                        type_path
                            .path
                            .segments
                            .insert(0, syn::PathSegment::from(syn::Token![super](span)));
                    }
                }
                syn::Type::BareFn(type_fn) => {
                    for arg in &mut type_fn.inputs {
                        inner(span, incompatible_items, &mut arg.ty);
                    }
                }
                other => unimplemented!("Unhandled type to reversion: {other:?}"),
            }
        }

        inner(span, incompatible_items, ty);
    };

    for item in vr_mod.content.unwrap().1 {
        match item {
            syn::Item::Impl(mut item) => {
                unversion_impl(&mut item);
                let syn::Type::Path(syn::TypePath { path, .. }) = &*item.self_ty else {
                    panic!("Impl block wasn't on a type path: {:#?}", *item.self_ty);
                };
                let Some(ident) = path.get_ident() else {
                    panic!("Impl block wasn't on ident: {path:?}");
                };
                let block_target = ident.to_string();
                let items = impl_map
                    .entry(ImplMapKey {
                        impl_type: block_target,
                        vr_mod: vr_mod.ident.to_string(),
                    })
                    .or_default();

                if let Some((_, path, _)) = item.trait_.as_mut() {
                    // unversion the generic parameters on the trait
                    for segment in path.segments.iter_mut() {
                        if let syn::PathArguments::AngleBracketed(
                            syn::AngleBracketedGenericArguments { args, .. },
                        ) = &mut segment.arguments
                        {
                            for arg in args.iter_mut() {
                                if let syn::GenericArgument::Type(ty) = arg {
                                    unversion_type(ty).unwrap();
                                }
                            }
                        }
                    }
                    items.push(ImplItem::TraitImpl(Box::new(item)));
                } else {
                    items.extend(item.items.into_iter().map(|i| ImplItem::Free(Box::new(i))));
                }
            }
            syn::Item::Enum(item) => {
                unversioned.insert(item.ident.to_string(), (item.into(), vr_mod.ident.clone()));
            }
            syn::Item::Union(mut item) => {
                unversion_fields(&mut item.fields.named);
                if (vr_mod.ident == "vr_0_9_12" || vr_mod.ident == "vr_0_9_20")
                    && item.ident == "VREvent_Data_t"
                {
                    for field in &mut item.fields.named {
                        reversion_type(&incompatible_items, &mut field.ty);
                    }
                    incompatible_items.insert(item.ident.to_string(), item.into());
                } else {
                    unversioned.insert(item.ident.to_string(), (item.into(), vr_mod.ident.clone()));
                }
            }
            syn::Item::Type(mut item) => {
                unversion_type(&mut item.ty).unwrap();
                unversioned.insert(item.ident.to_string(), (item.into(), vr_mod.ident.clone()));
            }
            syn::Item::Struct(mut item) => {
                static INCOMPAT_STRUCTS: &[(&str, &[&str])] = &[
                    (
                        "vr_0_9_12",
                        &["VREvent_t", "VREvent_Reserved_t", "Compositor_FrameTiming"],
                    ),
                    (
                        "vr_0_9_20",
                        &["VREvent_t", "VREvent_Reserved_t", "Compositor_FrameTiming"],
                    ),
                    ("vr_1_0_1", &["Compositor_FrameTiming"]),
                    ("vr_1_0_3", &["Compositor_FrameTiming"]),
                ];
                let should_reversion = INCOMPAT_STRUCTS
                    .iter()
                    .find(|(version, _)| vr_mod.ident == version)
                    .is_some_and(|(_, structs)| structs.contains(&item.ident.to_string().as_str()));

                for field in &mut item.fields {
                    unversion_type(&mut field.ty).unwrap();
                    if should_reversion {
                        reversion_type(&incompatible_items, &mut field.ty);
                    }
                }

                if should_reversion {
                    incompatible_items.insert(item.ident.to_string(), item.into());
                    continue;
                }
                match unversioned.entry(item.ident.to_string()) {
                    Entry::Vacant(e) => {
                        e.insert((item.into(), vr_mod.ident.clone()));
                    }
                    Entry::Occupied(mut e) => {
                        let (existing, e_mod) = e.get();
                        let syn::Item::Struct(existing) = &existing else {
                            unreachable!();
                        };
                        verify_fields_are_identical(
                            &item.ident,
                            &existing.fields,
                            e_mod,
                            &item.fields,
                            &vr_mod.ident,
                        );

                        // Replace if new item is superset of old item
                        if item.fields.len() > existing.fields.len() {
                            e.insert((item.into(), vr_mod.ident.clone()));
                        }
                    }
                }
            }
            syn::Item::Const(mut item) => {
                let name = item.ident.to_string();
                if name.ends_with("_Version") {
                    versionify_interface(
                        unversioned,
                        item,
                        &vr_mod.ident,
                        versioned,
                        &incompatible_items,
                    );
                } else {
                    unversion_type(&mut item.ty).unwrap();
                    unversioned.insert(item.ident.to_string(), (item.into(), vr_mod.ident.clone()));
                }
            }
            _ => {}
        }
    }

    if !incompatible_items.is_empty() {
        let vr_mod_ident = vr_mod.ident.clone();
        let items = incompatible_items.values();
        let incompat_mod = parse_quote! {
            pub mod #vr_mod_ident {
                #(#items)*
            }
        };
        unversioned.insert(vr_mod.ident.to_string(), (incompat_mod, vr_mod.ident));
    }
}
/// Returns pretty prineted file with types unified and versioned.
fn process_and_versionify_types(tokens: TokenStream) -> String {
    let file: syn::File = syn::parse2(tokens).unwrap();
    let mut unversioned = UnversionedItems::new();
    let mut versioned = VersionedInterfaces::new();
    let mut impl_items = ImplMap::new();
    let mut outer_items: Vec<syn::Item> = Vec::new();
    let mut outer_attrs: Vec<syn::Attribute> = Vec::new();

    let root_mod = file
        .items
        .into_iter()
        .find_map(|item| {
            if let syn::Item::Mod(m) = item {
                Some(m)
            } else {
                None
            }
        })
        .unwrap();

    let content = root_mod.content.unwrap().1;
    for item in content {
        match item {
            syn::Item::Mod(vr_mod) if vr_mod.ident.to_string().starts_with("vr") => {
                process_vr_namespace_content(
                    &mut unversioned,
                    &mut versioned,
                    &mut impl_items,
                    vr_mod,
                );
            }
            syn::Item::Use(item) => {
                if let syn::ItemUse {
                    attrs,
                    tree: syn::UseTree::Path(syn::UsePath { ident, .. }),
                    ..
                } = &item
                {
                    outer_attrs.extend_from_slice(attrs);
                    if ident == "self" {
                        continue;
                    }
                }
                outer_items.push(item.into());
            }
            syn::Item::Struct(_) | syn::Item::Macro(_) | syn::Item::Verbatim(_) => {
                outer_items.push(item)
            }
            syn::Item::Mod(_) => {}
            _ => todo!("unhandled item in root mod: {item:?}"),
        }
    }

    let versioned = versioned.into_iter().flat_map(|(_, mut versions)| {
        // reverse sort - start with highest interface version and go down
        versions.sort_by(|a, b| b.version.cmp(&a.version));

        let mut items = Vec::new();
        let mut prev_sigs = Vec::new();
        let mut prev_trait: Option<syn::Ident> = None;
        for VersionedInterface { item, vtable, .. } in versions {
            let GeneratedInterfaceData {
                gen_trait,
                gen_convert_trait,
                gen_mod,
            } = generate_vtable_trait(
                &item.ident.to_string(),
                &vtable,
                prev_trait.map(|ident| (ident, &prev_sigs)),
            );

            let if_impl: syn::Item = {
                let interface_ident = &item.ident;
                let vtable_ident = &vtable.ident;
                parse_quote! {
                    unsafe impl crate::OpenVrInterface for #interface_ident {
                        type Vtable = #vtable_ident;
                    }
                }
            };
            // save this trait to compare against the next trait
            prev_trait = Some(gen_trait.ident.clone());
            prev_sigs = gen_trait
                .items
                .iter()
                .filter_map(|item| match item {
                    syn::TraitItem::Fn(item) => Some(item.sig.clone()),
                    _ => None,
                })
                .collect();

            items.extend_from_slice(&[
                syn::Item::from(item),
                syn::Item::from(vtable),
                gen_trait.into(),
                gen_mod.into(),
                if_impl,
            ]);

            if let Some(item) = gen_convert_trait {
                items.push(item.into());
            }
        }

        items
    });
    let unversioned = unversioned
        .into_iter()
        .flat_map(|(name, (mut item, parent))| {
            // TODO: use the add_attributes method on ParseCallbacks instead
            // after updating bindgen?
            if let syn::Item::Enum(e) = &mut item
                && e.ident == "EVREventType"
            {
                e.attrs.push(parse_quote!(#[try_from(repr)]));
            }

            let mut items = vec![item];

            if let Some(impl_items) = impl_items.remove(&ImplMapKey {
                impl_type: name.clone(),
                vr_mod: parent.to_string(),
            }) {
                let mut free_items = Vec::new();
                items.extend(impl_items.into_iter().filter_map(|item| match item {
                    ImplItem::Free(item) => {
                        free_items.push(item);
                        None
                    }
                    ImplItem::TraitImpl(item) => Some((*item).into()),
                }));
                if !free_items.is_empty() {
                    let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
                    items.push(parse_quote! { impl #ident { #(#free_items)* }})
                }
            }

            items
        });

    let item_names = outer_items.iter().filter_map(|item| match item {
        syn::Item::Struct(s) => Some(syn::UseTree::Name(syn::UseName {
            ident: s.ident.clone(),
        })),
        _ => None,
    });
    let vr_uses: syn::UseTree = parse_quote! { {#(#item_names),*} };

    let attrs = root_mod.attrs;
    let m: syn::ItemMod = parse_quote! {
        #(#attrs)*
        #(#outer_attrs)*
        #[allow(dead_code)]
        #[allow(clippy::too_many_arguments)]
        #[allow(clippy::tabs_in_doc_comments)]
        #[allow(clippy::doc_lazy_continuation)]
        #[allow(clippy::upper_case_acronyms)]
        #[allow(clippy::manual_c_str_literals)]
        mod bindings {
            #(#outer_items)*
            pub mod vr {
                use super::#vr_uses;
                #(#unversioned)*
                #(#versioned)*
            }
        }
    };

    unparse(&syn::File {
        shebang: file.shebang,
        attrs: file.attrs,
        items: vec![m.into()],
    })
}

struct GeneratedInterfaceData {
    /// Generated interface trait
    gen_trait: syn::ItemTrait,
    /// Generated conversion trait
    gen_convert_trait: Option<syn::ItemTrait>,
    /// Generated interface module, for vtable/fntable dispatch
    gen_mod: syn::ItemMod,
}

/// Generates traits and other items for the given vtable.
/// The interface name is of the format "<interface><version>", i.e. "IVRSystem022"
/// The previous_trait is a trait to generate conversions to. It should have a version number
/// higher than the target trait.
fn generate_vtable_trait(
    interface_name: &str,
    vtable: &syn::ItemStruct,
    previous_trait: Option<(syn::Ident, &Vec<syn::Signature>)>,
) -> GeneratedInterfaceData {
    let interface_version_start_pos = interface_name.find(|c: char| c.is_numeric()).unwrap();
    let interface_ident = format_ident!("{interface_name}");
    let interface_no_version = &interface_name[0..interface_version_start_pos];
    let struct_prefix = format!("{interface_no_version}_");
    let trait_ident = format_ident!("{interface_name}_Interface");
    let mod_ident = format_ident!("{}", interface_name.to_lowercase());

    let mut trait_fns = Vec::new();
    // Functions for converting from this trait to the previous version
    let mut previous_compatible_fns = Vec::new();
    // Trait functions that require manually implementing conversion from this trait to the previous version
    let mut previous_incompatible_fns = Vec::new();
    // Functions to go in the module used for generating vtables.
    // These functions are generics and take a T implementing this interface.
    // Also includes the FnTable version of these functions
    let mut mod_fns = Vec::new();
    // The fields of the vtable, filled using the mod functions above
    let mut vtable_fields_init = Vec::new();
    // The fields and init of the generated fntable
    let mut fntable_fields = Vec::new();
    let mut fntable_init = Vec::new();

    let ty_to_fnarg = |ident: &syn::Ident, ty: &syn::Type| -> syn::FnArg {
        #[allow(clippy::useless_conversion, reason = "false positive")]
        syn::PatType::from(parse_quote!( #ident: #ty )).into()
    };

    let manual_conversion_trait_name = previous_trait.as_ref().map(|(name, _)| {
        let name = name.to_string();
        let previous_interface_version_start_pos = name.find(|c: char| c.is_numeric()).unwrap();
        let previous_interface_version_end_pos = name.find('_').unwrap();
        let previous_version =
            &name[previous_interface_version_start_pos..previous_interface_version_end_pos];
        format_ident!("{interface_name}On{previous_version}")
    });
    for field in &vtable.fields {
        let syn::Type::BareFn(f) = &field.ty else {
            panic!(
                "vtable {} has non BareFn field: {}",
                vtable.ident,
                field.to_token_stream()
            );
        };

        // strip struct prefix from fn
        let field_ident = field.ident.as_ref().unwrap();
        let fn_name = field_ident.to_string();
        let fn_name = fn_name.strip_prefix(&struct_prefix).unwrap_or_else(|| {
            panic!("function {fn_name} missing struct prefix ({struct_prefix})")
        });
        let fn_name = syn::Ident::new(fn_name, field.ident.as_ref().unwrap().span());
        let fn_output = &f.output;

        let fn_args = f
            .inputs
            .iter()
            .map(|arg| ty_to_fnarg(&arg.name.as_ref().unwrap().0, &arg.ty));

        let fn_args_names_only = f
            .inputs
            .iter()
            .skip(1)
            .map(|arg| &arg.name.as_ref().unwrap().0);

        let fn_enter_log: TokenStream = {
            let s = format!("Entered {interface_name}::{fn_name}");
            parse_quote! { log::trace!(target: "openvr_calls", #s); }
        };

        let bare: syn::ItemFn = {
            let params = fn_args.clone();
            let call_args = fn_args_names_only.clone();
            parse_quote! {
                extern "C" fn #fn_name<T: super::#trait_ident>(#(#params),*) #fn_output {
                    #[cfg(feature = "tracing")]
                    let _span = tracy_client::span!();
                    #fn_enter_log
                    let this = unsafe {
                        &*(this as *const _ as *const crate::VtableWrapper<super::#interface_ident, T>)
                    };
                    this.wrapped.upgrade().expect("Interface is no more!").#fn_name(#(#call_args),*)
                }
            }
        };
        mod_fns.push(syn::Item::from(bare));

        let fn_name_fntable = format_ident!("{fn_name}_FnTable");
        let fntable_fn: syn::ItemFn = {
            // FnTable versions of our vtables are missing the this parameter.
            let params = fn_args.clone().skip(1);
            let call_args = fn_args_names_only.clone();
            let fntable_init_err =
                format!("FnTable instance for {interface_name} was not initialized");
            let fntable_dead_err =
                format!("FnTable instance for {interface_name} has been destroyed");
            parse_quote! {
                extern "C" fn #fn_name_fntable(#(#params),*) #fn_output {
                    #[cfg(feature = "tracing")]
                    let _span = tracy_client::span!();
                    #fn_enter_log
                    let this = FNTABLE_INSTANCE
                        .instance
                        .read()
                        .unwrap()
                        .as_ref()
                        .expect(#fntable_init_err)
                        .upgrade()
                        .expect(#fntable_dead_err);

                    this.#fn_name(#(#call_args),*)
                }
            }
        };
        mod_fns.push(syn::Item::from(fntable_fn));

        let field: syn::FieldValue = parse_quote!(#field_ident: #fn_name::<T>);
        vtable_fields_init.push(field);
        let fntable_field: syn::Field = {
            // The FnTable version of the vtables are the same, except they omit the `this: *mut <interface>` argument
            let args_no_this = fn_args.clone().skip(1);
            parse_quote!(#field_ident: unsafe extern "C" fn(#(#args_no_this),*) #fn_output)
        };
        fntable_fields.push(fntable_field);
        let init: TokenStream = parse_quote!(#field_ident: #fn_name_fntable);
        fntable_init.push(init);

        let inputs: Vec<syn::FnArg> = f
            .inputs
            .iter()
            .map(|arg| {
                let ident = &arg.name.as_ref().unwrap().0;
                if ident == "this" {
                    // replace this pointer with self
                    let self_ty: syn::Receiver = parse_quote! { &self };
                    syn::FnArg::Receiver(self_ty)
                } else {
                    // pass through other types
                    ty_to_fnarg(ident, &arg.ty)
                }
            })
            .collect();

        let trait_fn: syn::TraitItemFn = parse_quote! {
            fn #fn_name(#(#inputs),*) #fn_output;
        };

        if let Some((_, previous_trait_fns)) = previous_trait {
            fn args_to_types<'a>(
                args: impl Iterator<Item = &'a syn::FnArg> + Clone,
            ) -> impl Iterator<Item = &'a Box<syn::Type>> + Clone {
                args.map(|arg| match arg {
                    syn::FnArg::Receiver(r) => &r.ty,
                    syn::FnArg::Typed(ty) => &ty.ty,
                })
            }

            let input_types = args_to_types(inputs.iter());
            let block: syn::Block = if !previous_trait_fns.iter().any(|prev_sig| {
                if prev_sig.ident != fn_name {
                    return false;
                }
                let arg_types = args_to_types(prev_sig.inputs.iter());
                arg_types.eq(input_types.clone())
            }) {
                previous_incompatible_fns.push(trait_fn.clone());
                let conv_trait = manual_conversion_trait_name.as_ref().unwrap();
                // In the event that we need our conversion interface, we need to disambiguate the
                // function call as using our conversion trait, because we'll have multiple things
                // with the same name.
                parse_quote! { { <Self as #conv_trait>::#fn_name(self, #(#fn_args_names_only),*) } }
            } else {
                parse_quote! { { self.#fn_name(#(#fn_args_names_only),*) } }
            };

            let compatible_fn = syn::TraitItemFn {
                attrs: vec![parse_quote! { #[inline] }],
                default: Some(block),
                ..trait_fn.clone()
            };
            previous_compatible_fns.push(compatible_fn);
        }
        trait_fns.push(trait_fn);
    }

    let incompat_trait: Option<syn::ItemTrait> =
        (!previous_incompatible_fns.is_empty()).then(|| {
            parse_quote! {
                pub trait #manual_conversion_trait_name {
                    #(#previous_incompatible_fns)*
                }
            }
        });

    let compat_impl: Option<syn::ItemImpl> = previous_trait.as_ref().map(|(prev_ident, _)| {
        let bounds: TokenStream = match incompat_trait.as_ref() {
            Some(t) => {
                let convert_trait = &t.ident;
                parse_quote! { impl<T: #prev_ident + #convert_trait> }
            }
            None => {
                parse_quote! { impl<T: #prev_ident> }
            }
        };
        parse_quote! {
            #bounds #trait_ident for T {
                #(#previous_compatible_fns)*
            }
        }
    });

    let vtable_ident = &vtable.ident;
    let version = {
        let v = &interface_name[interface_version_start_pos..];
        let s = format!("{interface_no_version}_{v}\0");
        proc_macro2::Literal::c_string(unsafe {
            std::ffi::CStr::from_bytes_with_nul_unchecked(s.as_bytes())
        })
    };
    GeneratedInterfaceData {
        gen_trait: parse_quote! {
            pub trait #trait_ident: Sync + Send + 'static {
                #(#trait_fns)*
            }
        },
        gen_convert_trait: incompat_trait,
        gen_mod: parse_quote! {
            mod #mod_ident {
                use super::*;
                use std::sync::{RwLock, Weak, Arc, LazyLock};
                use std::ffi::{CStr, c_void};
                use crate::{VtableWrapper, Inherits};

                impl #interface_ident {
                    pub const VERSION: &'static CStr = #version;
                }

                #[repr(C)]
                struct FnTable {
                    #(#fntable_fields,)*
                    instance: RwLock<Option<Weak<dyn #trait_ident>>>
                }

                impl Default for FnTable {
                    fn default() -> Self {
                        Self {
                            #(#fntable_init,)*
                            instance: RwLock::default()
                        }
                    }
                }
                static FNTABLE_INSTANCE: LazyLock<FnTable> = LazyLock::new(FnTable::default);

                unsafe impl<T: #trait_ident> Inherits<#interface_ident> for T {
                    fn new_wrapped(wrapped: &Arc<Self>) -> VtableWrapper<#interface_ident, Self> {
                        VtableWrapper {
                            base: #interface_ident {
                                vtable_: &#vtable_ident {
                                    #(#vtable_fields_init),*
                                }
                            },
                            wrapped: Arc::downgrade(wrapped)
                        }
                    }

                    fn init_fntable(init: &Arc<Self>) -> *mut c_void {
                        let mut instance = FNTABLE_INSTANCE.instance.write().unwrap();
                        if let Some(existing) = instance.as_mut().and_then(|e| e.upgrade()) {
                            let init: Arc<dyn #trait_ident> = init.clone();
                            assert!(Arc::ptr_eq(&existing, &init));
                        } else {
                            let init = Arc::downgrade(init);
                            *instance = Some(init);
                        }

                        &*FNTABLE_INSTANCE as *const FnTable as _
                    }
                }

                #(#mod_fns)*
                #compat_impl
            }
        },
    }
}
