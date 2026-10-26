//! Static checks for the explicitly supported Rust input lane.
use std::path::PathBuf;
use syn::visit::Visit;

fn reserved_namespace(name: &str) -> bool {
    matches!(
        name.trim_start_matches("r#"),
        "std" | "core" | "alloc" | "serde_json" | "serde" | "oasis7_wasm_sdk"
    )
}
fn known_macro_path(path: &syn::Path) -> bool {
    let parts = path
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [_] => true,
        [owner, name] => match name.as_str() {
            "export_wasm_module" => owner == "oasis7_wasm_sdk",
            "json" => owner == "serde_json",
            _ => matches!(owner.as_str(), "std" | "core" | "alloc"),
        },
        _ => false,
    }
}
fn unsafe_macro_import(tree: &syn::UseTree, owner: Option<&str>) -> bool {
    match tree {
        syn::UseTree::Path(path) => {
            unsafe_macro_import(&path.tree, owner.or(Some(&path.ident.to_string())))
        }
        syn::UseTree::Group(group) => group
            .items
            .iter()
            .any(|tree| unsafe_macro_import(tree, owner)),
        syn::UseTree::Glob(_) => !owner.is_some_and(|owner| {
            reserved_namespace(owner) || matches!(owner, "crate" | "self" | "super")
        }),
        syn::UseTree::Name(name) => {
            let name = name.ident.to_string();
            (known_macro(name.trim_start_matches("r#")) || reserved_namespace(&name))
                && !owner.is_some_and(|owner| {
                    reserved_namespace(owner) || matches!(owner, "crate" | "self" | "super")
                })
        }
        syn::UseTree::Rename(rename) => {
            let name = rename.rename.to_string();
            reserved_namespace(&name)
                || known_macro(name.trim_start_matches("r#"))
                    && !owner.is_some_and(reserved_namespace)
        }
    }
}

fn known_macro(name: &str) -> bool {
    matches!(
        name,
        "include"
            | "include_str"
            | "include_bytes"
            | "env"
            | "option_env"
            | "macro_rules"
            | "export_wasm_module"
            | "format"
            | "format_args"
            | "json"
            | "vec"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
            | "matches"
            | "panic"
            | "unreachable"
            | "todo"
            | "println"
            | "eprintln"
            | "print"
            | "eprint"
            | "write"
            | "writeln"
            | "cfg"
            | "stringify"
            | "concat"
            | "line"
            | "column"
            | "file"
    )
}

pub(super) struct Scanner {
    pub(super) includes: Vec<(String, bool)>,
    pub(super) reads: Vec<String>,
    pub(super) modules: Vec<(PathBuf, String)>,
    pub(super) module_dir: PathBuf,
    pub(super) error: Option<String>,
    pub(super) build: bool,
}
impl<'ast> Visit<'ast> for Scanner {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let name = node
            .path
            .segments
            .last()
            .map(|v| v.ident.to_string())
            .unwrap_or_default();
        // This lane does not evaluate arbitrary user/procedural macros. The SDK export
        // definition is itself in the local package closure and scanned for input tokens.
        if !known_macro(&name) || !known_macro_path(&node.path) {
            self.error = Some(format!("unsupported macro shape: {name}"));
        }
        if self.build
            && !matches!(
                name.as_str(),
                "include"
                    | "include_str"
                    | "include_bytes"
                    | "println"
                    | "eprintln"
                    | "assert"
                    | "assert_eq"
                    | "assert_ne"
                    | "panic"
                    | "cfg"
            )
        {
            self.error = Some("unsupported build-script macro/input".into());
        }
        if matches!(name.as_str(), "include" | "include_str" | "include_bytes") {
            match syn::parse2::<syn::LitStr>(node.tokens.clone()) {
                Ok(value) => self.includes.push((value.value(), name == "include")),
                Err(_) => self.error = Some("include requires a single literal path".into()),
            }
        } else if name == "macro_rules" && token_has_identifier(node.tokens.clone(), &["path"])
            || matches!(name.as_str(), "env" | "option_env")
            || contains_input_tokens(node.tokens.clone())
        {
            self.error = Some("environment/generated or nested macro input is unsupported".into());
        }
        if self.build && name == "println" {
            use syn::parse::Parser;
            let args = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                .parse2(node.tokens.clone());
            let safe=args.is_ok_and(|args| args.is_empty() || args.len()==1 && matches!(args.first(),Some(syn::Expr::Lit(syn::ExprLit{lit:syn::Lit::Str(value),..})) if !value.value().contains(['{','}']) && !value.value().lines().any(|line|line.trim_start().starts_with("cargo:"))));
            if !safe {
                self.error=Some("unsupported build stdout/Cargo directive; only static ordinary diagnostics are supported".into());
            }
        }
        if self.build
            && matches!(
                name.as_str(),
                "println" | "eprintln" | "assert" | "assert_eq" | "assert_ne" | "panic"
            )
        {
            use syn::parse::Parser;
            match syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                .parse2(node.tokens.clone())
            {
                Ok(args) => {
                    for expr in args {
                        self.visit_expr(&expr);
                    }
                }
                Err(_) => self.error = Some("unsupported build-script macro arguments".into()),
            }
        }
        syn::visit::visit_macro(self, node);
    }
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if node
            .ident
            .as_ref()
            .is_some_and(|name| name != "export_wasm_module")
        {
            self.error = Some("unsupported user macro definition".into());
        }
        syn::visit::visit_item_macro(self, node);
    }
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if self.build || unsafe_macro_import(&node.tree, None) {
            self.error = Some("unsupported macro-owner import or build namespace alias".into());
        }
        syn::visit::visit_item_use(self, node);
    }
    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        if self.build
            || node
                .rename
                .as_ref()
                .is_some_and(|(_, name)| reserved_namespace(&name.to_string()))
        {
            self.error = Some("unsupported extern crate or macro-owner alias".into());
        }
        syn::visit::visit_item_extern_crate(self, node);
    }
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if self.build || reserved_namespace(&node.ident.to_string()) {
            self.error = Some("unsupported build module or macro-owner shadow".into());
        }
        if node.content.is_none() {
            self.modules
                .push((self.module_dir.clone(), node.ident.to_string()));
        }
        let previous = self.module_dir.clone();
        self.module_dir.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.module_dir = previous;
    }
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if !known_attribute(&node.meta) {
            self.error = Some("unsupported attribute/derive/path input shape".into());
        }
        syn::visit::visit_attribute(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.build && !matches!(node.method.to_string().as_str(), "unwrap" | "expect") {
            self.error = Some("unsupported build-script method/input".into());
        }
        syn::visit::visit_expr_method_call(self, node);
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.build {
            let allowed = if let syn::Expr::Path(path) = &*node.func {
                let name = path
                    .path
                    .segments
                    .iter()
                    .map(|v| v.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                if matches!(
                    name.as_str(),
                    "std::fs::read" | "std::fs::read_to_string" | "std::fs::File::open"
                ) {
                    if let Some(syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(value),
                        ..
                    })) = node.args.first()
                    {
                        self.reads.push(value.value());
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if !allowed {
                self.error = Some(
                    "build script supports only explicitly declared literal file reads".into(),
                );
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}

fn token_has_identifier(tokens: proc_macro2::TokenStream, names: &[&str]) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(value) => names.contains(&value.to_string().as_str()),
        proc_macro2::TokenTree::Group(group) => token_has_identifier(group.stream(), names),
        _ => false,
    })
}
fn contains_input_tokens(tokens: proc_macro2::TokenStream) -> bool {
    if token_has_identifier(
        tokens.clone(),
        &[
            "include",
            "include_str",
            "include_bytes",
            "env",
            "option_env",
            "mod",
            "use",
        ],
    ) {
        return true;
    }
    let tokens = tokens.into_iter().collect::<Vec<_>>();
    for (index, window) in tokens.windows(3).enumerate() {
        if let [
            proc_macro2::TokenTree::Ident(name),
            proc_macro2::TokenTree::Punct(mark),
            proc_macro2::TokenTree::Group(_),
        ] = window
            && mark.as_char() == '!'
            && name != "if"
            && (!known_macro(&name.to_string())
                || index >= 3
                    && matches!((&tokens[index-3],&tokens[index-2],&tokens[index-1]),(proc_macro2::TokenTree::Ident(owner),proc_macro2::TokenTree::Punct(a),proc_macro2::TokenTree::Punct(b)) if a.as_char()==':' && b.as_char()==':' && !reserved_namespace(&owner.to_string())))
        {
            return true;
        }
    }
    tokens.into_iter().any(|token|matches!(token,proc_macro2::TokenTree::Group(group) if contains_input_tokens(group.stream())))
}
fn known_attribute(meta: &syn::Meta) -> bool {
    use syn::parse::Parser;
    let path = meta.path();
    if path
        .segments
        .first()
        .is_some_and(|segment| matches!(segment.ident.to_string().as_str(), "rustfmt" | "clippy"))
    {
        return true;
    }
    let Some(name) = path.get_ident().map(ToString::to_string) else {
        return false;
    };
    match name.as_str() {
        "cfg_attr" => {
            let syn::Meta::List(list) = meta else {
                return false;
            };
            if token_has_identifier(
                list.tokens.clone(),
                &[
                    "path",
                    "include",
                    "include_str",
                    "include_bytes",
                    "env",
                    "option_env",
                ],
            ) {
                return false;
            }
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .is_ok_and(|values| values.len() >= 2 && values.iter().skip(1).all(known_attribute))
        }
        "derive" => {
            let syn::Meta::List(list) = meta else {
                return false;
            };
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .is_ok_and(|values| {
                    values.iter().all(|path| {
                        path.segments.last().is_some_and(|part| {
                            matches!(
                                part.ident.to_string().as_str(),
                                "Clone"
                                    | "Copy"
                                    | "Debug"
                                    | "Default"
                                    | "Eq"
                                    | "PartialEq"
                                    | "Ord"
                                    | "PartialOrd"
                                    | "Hash"
                                    | "Serialize"
                                    | "Deserialize"
                            )
                        }) && (path.segments.len() == 1
                            || path.segments.first().is_some_and(|part| {
                                matches!(part.ident.to_string().as_str(), "std" | "core" | "serde")
                            }))
                    })
                })
        }
        "unsafe" => {
            let syn::Meta::List(list) = meta else {
                return false;
            };
            if contains_input_tokens(list.tokens.clone()) {
                return false;
            }
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .is_ok_and(|values| values.iter().all(known_attribute))
        }
        "link" => {
            let syn::Meta::List(list) = meta else {
                return false;
            };
            syn::punctuated::Punctuated::<syn::Meta,syn::Token![,]>::parse_terminated.parse2(list.tokens.clone()).is_ok_and(|values|values.iter().all(|value|matches!(value,syn::Meta::NameValue(value) if value.path.is_ident("wasm_import_module") && matches!(&value.value,syn::Expr::Lit(syn::ExprLit{lit:syn::Lit::Str(_),..})))))
        }
        "cfg"
        | "serde"
        | "allow"
        | "warn"
        | "deny"
        | "forbid"
        | "expect"
        | "repr"
        | "no_std"
        | "no_main"
        | "no_mangle"
        | "export_name"
        | "link_name"
        | "link_section"
        | "doc"
        | "inline"
        | "cold"
        | "must_use"
        | "deprecated"
        | "non_exhaustive"
        | "test"
        | "ignore"
        | "should_panic"
        | "macro_export"
        | "automatically_derived"
        | "global_allocator"
        | "used"
        | "track_caller" => true,
        _ => false,
    }
}
