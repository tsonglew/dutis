use crate::association::{AssociationKind, AssociationTarget, HandlerRole};
use anyhow::{Context, Result};
use plist::{Dictionary, Value};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Default)]
pub struct PlistParser;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AppMetadata {
    pub bundle_id: Option<String>,
    pub extensions: Vec<String>,
    pub handlers: Vec<DeclaredHandler>,
    pub type_declarations: Vec<TypeDeclaration>,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HandlerDeclarationSource {
    DocumentType,
    UrlType,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DeclaredHandler {
    pub kind: AssociationKind,
    pub identifier: String,
    pub role: HandlerRole,
    pub source: HandlerDeclarationSource,
}

impl DeclaredHandler {
    pub fn association(&self) -> AssociationTarget {
        AssociationTarget {
            kind: self.kind,
            identifier: self.identifier.clone(),
            role: self.role,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeDeclarationSource {
    Exported,
    Imported,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TypeDeclaration {
    pub identifier: String,
    pub source: TypeDeclarationSource,
    pub conforms_to: Vec<String>,
    pub extensions: Vec<String>,
    pub mime_types: Vec<String>,
}

impl PlistParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse_metadata(&self, plist_path: &Path) -> Result<AppMetadata> {
        let plist = Value::from_file(plist_path)
            .with_context(|| format!("failed to parse {}", plist_path.display()))?;
        let handlers = extract_handlers(&plist);
        let type_declarations = extract_type_declarations(&plist);
        Ok(AppMetadata {
            bundle_id: plist
                .as_dictionary()
                .and_then(|root| root.get("CFBundleIdentifier"))
                .and_then(Value::as_string)
                .map(str::to_owned),
            extensions: extract_extensions(&plist),
            handlers,
            type_declarations,
        })
    }
}

fn extract_extensions(plist: &Value) -> Vec<String> {
    let mut extensions = BTreeSet::new();
    let Some(root) = plist.as_dictionary() else {
        return Vec::new();
    };

    collect_document_type_extensions(root, &mut extensions);
    collect_type_declaration_extensions(root, "UTExportedTypeDeclarations", &mut extensions);
    collect_type_declaration_extensions(root, "UTImportedTypeDeclarations", &mut extensions);
    extensions.into_iter().collect()
}

fn extract_handlers(plist: &Value) -> Vec<DeclaredHandler> {
    let mut handlers = BTreeSet::new();
    let Some(root) = plist.as_dictionary() else {
        return Vec::new();
    };
    collect_document_type_handlers(root, &mut handlers);
    collect_url_type_handlers(root, &mut handlers);
    handlers.into_iter().collect()
}

fn collect_document_type_handlers(root: &Dictionary, handlers: &mut BTreeSet<DeclaredHandler>) {
    let Some(document_types) = root.get("CFBundleDocumentTypes").and_then(Value::as_array) else {
        return;
    };
    for document_type in document_types.iter().filter_map(Value::as_dictionary) {
        let Some(role) = document_handler_role(document_type) else {
            continue;
        };
        collect_handler_values(
            document_type.get("CFBundleTypeExtensions"),
            AssociationKind::Extension,
            role,
            HandlerDeclarationSource::DocumentType,
            handlers,
        );
        collect_handler_values(
            document_type.get("LSItemContentTypes"),
            AssociationKind::Uti,
            role,
            HandlerDeclarationSource::DocumentType,
            handlers,
        );
        collect_handler_values(
            document_type.get("CFBundleTypeMIMETypes"),
            AssociationKind::Mime,
            role,
            HandlerDeclarationSource::DocumentType,
            handlers,
        );
    }
}

fn collect_url_type_handlers(root: &Dictionary, handlers: &mut BTreeSet<DeclaredHandler>) {
    let Some(url_types) = root.get("CFBundleURLTypes").and_then(Value::as_array) else {
        return;
    };
    for url_type in url_types.iter().filter_map(Value::as_dictionary) {
        collect_handler_values(
            url_type.get("CFBundleURLSchemes"),
            AssociationKind::UrlScheme,
            HandlerRole::All,
            HandlerDeclarationSource::UrlType,
            handlers,
        );
    }
}

fn document_handler_role(document_type: &Dictionary) -> Option<HandlerRole> {
    match document_type
        .get("CFBundleTypeRole")
        .and_then(Value::as_string)
        .map(str::trim)
    {
        None | Some("") => Some(HandlerRole::All),
        Some(value) if value.eq_ignore_ascii_case("viewer") => Some(HandlerRole::Viewer),
        Some(value) if value.eq_ignore_ascii_case("editor") => Some(HandlerRole::Editor),
        Some(value) if value.eq_ignore_ascii_case("shell") => Some(HandlerRole::Shell),
        Some(value) if value.eq_ignore_ascii_case("none") => None,
        Some(_) => None,
    }
}

fn collect_handler_values(
    value: Option<&Value>,
    kind: AssociationKind,
    role: HandlerRole,
    source: HandlerDeclarationSource,
    handlers: &mut BTreeSet<DeclaredHandler>,
) {
    for value in string_values(value) {
        let Ok(target) = AssociationTarget::new(kind, value, role) else {
            continue;
        };
        handlers.insert(DeclaredHandler {
            kind: target.kind,
            identifier: target.identifier,
            role: target.role,
            source,
        });
    }
}

fn extract_type_declarations(plist: &Value) -> Vec<TypeDeclaration> {
    let mut declarations = BTreeSet::new();
    let Some(root) = plist.as_dictionary() else {
        return Vec::new();
    };
    collect_type_declarations(
        root,
        "UTExportedTypeDeclarations",
        TypeDeclarationSource::Exported,
        &mut declarations,
    );
    collect_type_declarations(
        root,
        "UTImportedTypeDeclarations",
        TypeDeclarationSource::Imported,
        &mut declarations,
    );
    declarations.into_iter().collect()
}

fn collect_type_declarations(
    root: &Dictionary,
    key: &str,
    source: TypeDeclarationSource,
    declarations: &mut BTreeSet<TypeDeclaration>,
) {
    let Some(values) = root.get(key).and_then(Value::as_array) else {
        return;
    };
    for declaration in values.iter().filter_map(Value::as_dictionary) {
        let Some(identifier) = declaration
            .get("UTTypeIdentifier")
            .and_then(Value::as_string)
            .and_then(|value| {
                AssociationTarget::new(AssociationKind::Uti, value, HandlerRole::All).ok()
            })
            .map(|target| target.identifier)
        else {
            continue;
        };
        let conforms_to =
            normalized_values(declaration.get("UTTypeConformsTo"), AssociationKind::Uti);
        let tags = declaration
            .get("UTTypeTagSpecification")
            .and_then(Value::as_dictionary);
        let extensions = tags
            .map(|tags| {
                normalized_values(
                    tags.get("public.filename-extension"),
                    AssociationKind::Extension,
                )
            })
            .unwrap_or_default();
        let mime_types = tags
            .map(|tags| normalized_values(tags.get("public.mime-type"), AssociationKind::Mime))
            .unwrap_or_default();
        declarations.insert(TypeDeclaration {
            identifier,
            source,
            conforms_to,
            extensions,
            mime_types,
        });
    }
}

fn normalized_values(value: Option<&Value>, kind: AssociationKind) -> Vec<String> {
    string_values(value)
        .filter_map(|value| AssociationTarget::new(kind, value, HandlerRole::All).ok())
        .map(|target| target.identifier)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn string_values(value: Option<&Value>) -> impl Iterator<Item = &str> {
    let values = match value {
        Some(Value::String(value)) => vec![value.as_str()],
        Some(Value::Array(values)) => values.iter().filter_map(Value::as_string).collect(),
        _ => Vec::new(),
    };
    values.into_iter()
}

fn collect_document_type_extensions(root: &Dictionary, extensions: &mut BTreeSet<String>) {
    let Some(document_types) = root.get("CFBundleDocumentTypes").and_then(Value::as_array) else {
        return;
    };

    for document_type in document_types.iter().filter_map(Value::as_dictionary) {
        if let Some(values) = document_type.get("CFBundleTypeExtensions") {
            collect_string_values(values, extensions);
        }
    }
}

fn collect_type_declaration_extensions(
    root: &Dictionary,
    key: &str,
    extensions: &mut BTreeSet<String>,
) {
    let Some(declarations) = root.get(key).and_then(Value::as_array) else {
        return;
    };

    for declaration in declarations.iter().filter_map(Value::as_dictionary) {
        let Some(tags) = declaration
            .get("UTTypeTagSpecification")
            .and_then(Value::as_dictionary)
        else {
            continue;
        };
        if let Some(values) = tags.get("public.filename-extension") {
            collect_string_values(values, extensions);
        }
    }
}

fn collect_string_values(value: &Value, extensions: &mut BTreeSet<String>) {
    match value {
        Value::String(value) => insert_extension(value, extensions),
        Value::Array(values) => {
            for value in values.iter().filter_map(Value::as_string) {
                insert_extension(value, extensions);
            }
        }
        _ => {}
    }
}

fn insert_extension(value: &str, extensions: &mut BTreeSet<String>) {
    let extension = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if !extension.is_empty() && extension != "*" {
        extensions.insert(extension);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictionary(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Dictionary(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    #[test]
    fn extracts_legacy_and_modern_extension_declarations() {
        let plist = dictionary([
            (
                "CFBundleDocumentTypes",
                Value::Array(vec![dictionary([(
                    "CFBundleTypeExtensions",
                    Value::Array(vec![Value::String("TXT".into()), Value::String("*".into())]),
                )])]),
            ),
            (
                "UTExportedTypeDeclarations",
                Value::Array(vec![dictionary([(
                    "UTTypeTagSpecification",
                    dictionary([(
                        "public.filename-extension",
                        Value::Array(vec![
                            Value::String(".md".into()),
                            Value::String("txt".into()),
                        ]),
                    )]),
                )])]),
            ),
        ]);

        assert_eq!(extract_extensions(&plist), vec!["md", "txt"]);
    }

    #[test]
    fn accepts_a_single_extension_string() {
        let plist = dictionary([(
            "CFBundleDocumentTypes",
            Value::Array(vec![dictionary([(
                "CFBundleTypeExtensions",
                Value::String("json".into()),
            )])]),
        )]);

        assert_eq!(extract_extensions(&plist), vec!["json"]);
    }

    #[test]
    fn extracts_bundle_identifier_with_extensions() {
        let plist = dictionary([
            (
                "CFBundleIdentifier",
                Value::String("com.example.Editor".into()),
            ),
            (
                "CFBundleDocumentTypes",
                Value::Array(vec![dictionary([(
                    "CFBundleTypeExtensions",
                    Value::String("txt".into()),
                )])]),
            ),
        ]);
        let root = plist.as_dictionary().unwrap();
        assert_eq!(
            root.get("CFBundleIdentifier").and_then(Value::as_string),
            Some("com.example.Editor")
        );
        assert_eq!(extract_extensions(&plist), vec!["txt"]);
    }

    #[test]
    fn extracts_role_aware_document_and_url_handlers() {
        let plist = dictionary([
            (
                "CFBundleDocumentTypes",
                Value::Array(vec![
                    dictionary([
                        ("CFBundleTypeRole", Value::String("Editor".into())),
                        (
                            "CFBundleTypeExtensions",
                            Value::Array(vec![Value::String("MD".into())]),
                        ),
                        (
                            "LSItemContentTypes",
                            Value::Array(vec![Value::String("Public.Text".into())]),
                        ),
                        (
                            "CFBundleTypeMIMETypes",
                            Value::Array(vec![Value::String("Text/Plain".into())]),
                        ),
                    ]),
                    dictionary([
                        ("CFBundleTypeRole", Value::String("None".into())),
                        ("CFBundleTypeExtensions", Value::String("pdf".into())),
                    ]),
                ]),
            ),
            (
                "CFBundleURLTypes",
                Value::Array(vec![dictionary([(
                    "CFBundleURLSchemes",
                    Value::Array(vec![Value::String("HTTPS".into())]),
                )])]),
            ),
        ]);

        let handlers = extract_handlers(&plist);
        assert_eq!(handlers.len(), 4);
        assert_eq!(handlers[0].kind, AssociationKind::Extension);
        assert_eq!(handlers[0].identifier, "md");
        assert_eq!(handlers[0].role, HandlerRole::Editor);
        assert_eq!(handlers[1].kind, AssociationKind::Uti);
        assert_eq!(handlers[1].identifier, "public.text");
        assert_eq!(handlers[2].kind, AssociationKind::Mime);
        assert_eq!(handlers[2].identifier, "text/plain");
        assert_eq!(handlers[3].kind, AssociationKind::UrlScheme);
        assert_eq!(handlers[3].identifier, "https");
        assert_eq!(handlers[3].role, HandlerRole::All);
    }

    #[test]
    fn keeps_imported_and_exported_type_definitions_separate_from_handlers() {
        let declaration = |identifier: &str| {
            dictionary([
                ("UTTypeIdentifier", Value::String(identifier.into())),
                (
                    "UTTypeConformsTo",
                    Value::Array(vec![Value::String("Public.Text".into())]),
                ),
                (
                    "UTTypeTagSpecification",
                    dictionary([
                        (
                            "public.filename-extension",
                            Value::Array(vec![Value::String("Example".into())]),
                        ),
                        (
                            "public.mime-type",
                            Value::Array(vec![Value::String("Text/X-Example".into())]),
                        ),
                    ]),
                ),
            ])
        };
        let plist = dictionary([
            (
                "UTExportedTypeDeclarations",
                Value::Array(vec![declaration("Com.Example.Exported")]),
            ),
            (
                "UTImportedTypeDeclarations",
                Value::Array(vec![declaration("Com.Example.Imported")]),
            ),
        ]);

        assert!(extract_handlers(&plist).is_empty());
        let declarations = extract_type_declarations(&plist);
        assert_eq!(declarations.len(), 2);
        assert_eq!(declarations[0].identifier, "com.example.exported");
        assert_eq!(declarations[0].source, TypeDeclarationSource::Exported);
        assert_eq!(declarations[0].conforms_to, ["public.text"]);
        assert_eq!(declarations[0].extensions, ["example"]);
        assert_eq!(declarations[0].mime_types, ["text/x-example"]);
        assert_eq!(declarations[1].source, TypeDeclarationSource::Imported);
    }
}
