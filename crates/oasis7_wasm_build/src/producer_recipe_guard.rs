//! Bounded production-module/input guard for the two known native producer packages.
use super::*;
use syn::visit::Visit;

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test") || matches!(&attr.meta,syn::Meta::List(list) if list.path.is_ident("cfg") && list.tokens.to_string()=="test")
    })
}
fn in_producer(root: &Path, file: &Path) -> bool {
    PRODUCERS
        .iter()
        .any(|path| file.starts_with(root.join(path).join("src")))
}
fn shadows_syn(tree: &syn::UseTree) -> bool {
    match tree {
        syn::UseTree::Name(name) => name.ident == "syn",
        syn::UseTree::Rename(rename) => rename.rename == "syn",
        // These modules inherit only names from their own already-scanned parent.
        // External and deeper wildcard reexports remain unsupported.
        syn::UseTree::Path(path)
            if path.ident == "super" && matches!(*path.tree, syn::UseTree::Glob(_)) =>
        {
            false
        }
        syn::UseTree::Path(path) => shadows_syn(&path.tree),
        syn::UseTree::Group(group) => group.items.iter().any(shadows_syn),
        syn::UseTree::Glob(_) => true,
    }
}

struct Guard<'a> {
    root: &'a Path,
    file: &'a Path,
    directory: PathBuf,
    modules: Vec<(PathBuf, bool)>,
    error: Option<String>,
}
impl<'ast> Visit<'ast> for Guard<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let attrs = match item {
            syn::Item::Mod(v) => &v.attrs,
            syn::Item::Fn(v) => &v.attrs,
            syn::Item::Const(v) => &v.attrs,
            syn::Item::Static(v) => &v.attrs,
            syn::Item::Impl(v) => &v.attrs,
            syn::Item::Struct(v) => &v.attrs,
            syn::Item::Enum(v) => &v.attrs,
            syn::Item::Use(v) => &v.attrs,
            _ => return syn::visit::visit_item(self, item),
        };
        if !test_only(attrs) {
            syn::visit::visit_item(self, item);
        }
    }
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if !attr.path().is_ident("path") && !crate::source_inputs::scan::known_attribute(&attr.meta)
        {
            self.error = Some("unsupported producer attribute/input".into());
        }
        syn::visit::visit_attribute(self, attr);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        for attr in &item.attrs {
            self.visit_attribute(attr);
        }
        if shadows_syn(&item.tree)
            || crate::source_inputs::scan::unsafe_macro_import(&item.tree, None)
        {
            self.error = Some("unsupported producer macro-owner import".into());
        }
    }
    fn visit_item_extern_crate(&mut self, _: &'ast syn::ItemExternCrate) {
        self.error = Some("unsupported producer extern crate".into());
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        for attr in &item.attrs {
            self.visit_attribute(attr);
        }
        if item.ident.is_some() {
            self.error = Some("unsupported producer macro definition".into());
        } else {
            self.visit_macro(&item.mac);
        }
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if test_only(&item.attrs) {
            return;
        }
        if item.ident == "syn"
            || crate::source_inputs::scan::reserved_namespace(&item.ident.to_string())
        {
            self.error = Some("unsupported producer macro-owner shadow".into());
            return;
        }
        for attr in &item.attrs {
            self.visit_attribute(attr);
        }
        if let Some((_, items)) = &item.content {
            let previous = self.directory.clone();
            self.directory.push(item.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.directory = previous;
            return;
        }
        let explicit = item.attrs.iter().find(|attr| attr.path().is_ident("path"));
        let next = if let Some(attr) = explicit {
            if let syn::Meta::NameValue(value) = &attr.meta
                && let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(value),
                    ..
                }) = &value.value
            {
                include_path(self.root, self.file, &value.value()).ok()
            } else {
                None
            }
        } else {
            let plain = self.directory.join(format!("{}.rs", item.ident));
            let nested = self.directory.join(item.ident.to_string()).join("mod.rs");
            match (plain.is_file(), nested.is_file()) {
                (true, false) => Some(plain),
                (false, true) => Some(nested),
                _ => None,
            }
        };
        match next {
            Some(path) if in_producer(self.root, &path) => self.modules.push((path, false)),
            _ => self.error = Some(
                "producer module path is missing, ambiguous or outside declared package sources"
                    .into(),
            ),
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let name = mac
            .path
            .segments
            .last()
            .map(|part| part.ident.to_string())
            .unwrap_or_default();
        if mac
            .path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect::<Vec<_>>()
            == ["syn", "Token"]
            && mac.tokens.to_string() == ","
        {
            return;
        }
        if matches!(name.as_str(), "env" | "option_env") {
            if !syn::parse2::<syn::LitStr>(mac.tokens.clone())
                .is_ok_and(|value| value.value() == "CARGO_PKG_VERSION")
            {
                self.error = Some("unsupported producer compile-time environment input".into());
            }
            return;
        }
        if matches!(name.as_str(), "include" | "include_str" | "include_bytes") {
            let path = syn::parse2::<syn::LitStr>(mac.tokens.clone())
                .ok()
                .and_then(|value| include_path(self.root, self.file, &value.value()).ok());
            if name == "include"
                || !path.as_ref().is_some_and(|path| {
                    in_producer(self.root, path) && validate_file(self.root, path).is_ok()
                })
            {
                self.error = Some("unsupported producer literal include/input".into());
            }
            return;
        }
        if !crate::source_inputs::scan::known_macro(&name)
            || !crate::source_inputs::scan::known_macro_path(&mac.path)
            || crate::source_inputs::scan::contains_input_tokens(mac.tokens.clone())
        {
            self.error = Some(format!(
                "unsupported producer macro/input {name}: {}",
                mac.tokens
            ));
        }
    }
}

pub(super) fn walk(
    root: &Path,
    file: &Path,
    crate_root: bool,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), SourceHashError> {
    if !visited.insert(file.into()) {
        return Ok(());
    }
    validate_file(root, file)?;
    let text = fs::read_to_string(file).map_err(|source| SourceHashError::Io {
        path: Some(file.into()),
        source,
    })?;
    let syntax = syn::parse_file(&text).map_err(invalid)?;
    let parent = file
        .parent()
        .ok_or_else(|| invalid("producer file lacks parent"))?;
    let directory = if crate_root || file.file_name().is_some_and(|name| name == "mod.rs") {
        parent.into()
    } else {
        parent.join(
            file.file_stem()
                .ok_or_else(|| invalid("producer file lacks stem"))?,
        )
    };
    let mut guard = Guard {
        root,
        file,
        directory,
        modules: Vec::new(),
        error: None,
    };
    guard.visit_file(&syntax);
    if let Some(error) = guard.error {
        return Err(invalid(format!("{}: {error}", file.display())));
    }
    for (path, is_root) in guard.modules {
        walk(root, &path, is_root, visited)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(text: &str) -> Option<String> {
        let root = Path::new("/producer");
        let file = root.join("tools/wasm_build_suite/src/lib.rs");
        let mut guard = Guard {
            root,
            file: &file,
            directory: file.parent().unwrap().into(),
            modules: Vec::new(),
            error: None,
        };
        guard.visit_file(&syn::parse_file(text).unwrap());
        guard.error
    }

    #[test]
    fn native_syn_token_lane_rejects_lexical_owner_shadows() {
        assert!(scan("type Comma = syn::Token![,];").is_none());
        for text in [
            "use foreign as syn; type Comma = syn::Token![,];",
            "use foreign::{owner as syn}; type Comma = syn::Token![,];",
            "mod syn {} type Comma = syn::Token![,];",
            "use foreign::*; type Comma = syn::Token![,];",
        ] {
            assert!(scan(text).is_some(), "accepted shadow: {text}");
        }
    }
    #[test]
    fn standard_attribute_values_are_checked_on_functions_imports_and_macro_items() {
        let mut admitted = Vec::new();
        for text in [
            "#[doc = include_str!(\"../../../external.txt\")] pub fn value() {}",
            "#[doc = include_str!(\"../../../external.txt\")] use std::fmt;",
            "#[doc = include_str!(\"../../../external.txt\")] println!(\"literal\");",
            "#[cfg_attr(any(), doc = include_str!(\"../../../external.txt\"))] pub fn value() {}",
        ] {
            if scan(text).is_none() {
                admitted.push(text);
            }
        }
        assert!(
            admitted.is_empty(),
            "standard attribute input skipped: {admitted:?}"
        );
    }
}
