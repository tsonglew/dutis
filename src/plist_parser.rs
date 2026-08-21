use anyhow::{Context, Result};
use plist::{Dictionary, Value};
use std::collections::BTreeSet;
use std::path::Path;

pub struct PlistParser;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AppMetadata {
    pub bundle_id: Option<String>,
    pub extensions: Vec<String>,
}

impl PlistParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse_metadata(&self, plist_path: &Path) -> Result<AppMetadata> {
        let plist = Value::from_file(plist_path)
            .with_context(|| format!("failed to parse {}", plist_path.display()))?;
        Ok(AppMetadata {
            bundle_id: plist
                .as_dictionary()
                .and_then(|root| root.get("CFBundleIdentifier"))
                .and_then(Value::as_string)
                .map(str::to_owned),
            extensions: extract_extensions(&plist),
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
}
