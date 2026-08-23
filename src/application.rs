use crate::app_scanner::AppScanner;
use crate::association::{AssociationTarget, HandlerRole};
use crate::plist_parser::{DeclaredHandler, PlistParser, TypeDeclaration};
use anyhow::{bail, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct Application {
    pub name: String,
    pub path: PathBuf,
    pub bundle_id: Option<String>,
    pub extensions: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub handlers: Vec<DeclaredHandler>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub type_declarations: Vec<TypeDeclaration>,
}

#[derive(Debug)]
pub struct ApplicationCatalog {
    pub applications: Vec<Application>,
    pub metadata_failures: usize,
}

impl ApplicationCatalog {
    pub fn scan() -> Result<Self> {
        let installed_apps = AppScanner::new().scan_applications()?;
        let parser = PlistParser::new();
        let mut metadata_failures = 0;
        let applications = installed_apps
            .into_iter()
            .map(|installed| {
                let plist_path = installed.path.join("Contents/Info.plist");
                let metadata = parser.parse_metadata(&plist_path);
                if metadata.is_err() {
                    metadata_failures += 1;
                }
                let (bundle_id, extensions, handlers, type_declarations) = match metadata {
                    Ok(metadata) => (
                        metadata.bundle_id,
                        metadata.extensions,
                        metadata.handlers,
                        metadata.type_declarations,
                    ),
                    Err(_) => (None, Vec::new(), Vec::new(), Vec::new()),
                };
                Application {
                    name: installed.name,
                    path: installed.path,
                    bundle_id,
                    extensions,
                    handlers,
                    type_declarations,
                }
            })
            .collect();

        Ok(Self {
            applications,
            metadata_failures,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct HandlerCandidate<'a> {
    pub name: &'a str,
    pub path: &'a Path,
    pub bundle_id: Option<&'a str>,
    pub declarations: Vec<&'a DeclaredHandler>,
}

pub fn find_handler_candidates<'a>(
    applications: &'a [Application],
    association: &AssociationTarget,
) -> Vec<HandlerCandidate<'a>> {
    applications
        .iter()
        .filter_map(|application| {
            let declarations = application
                .handlers
                .iter()
                .filter(|handler| {
                    handler.kind == association.kind
                        && handler.identifier == association.identifier
                        && role_supports(handler.role, association.role)
                })
                .collect::<Vec<_>>();
            (!declarations.is_empty()).then_some(HandlerCandidate {
                name: &application.name,
                path: &application.path,
                bundle_id: application.bundle_id.as_deref(),
                declarations,
            })
        })
        .collect()
}

fn role_supports(declared: HandlerRole, requested: HandlerRole) -> bool {
    declared == HandlerRole::All
        || requested == HandlerRole::All
        || declared == requested
        || (requested == HandlerRole::Viewer && declared == HandlerRole::Editor)
}

pub fn find_apps_for_extension<'a>(
    applications: &'a [Application],
    extension: &str,
) -> Vec<&'a Application> {
    applications
        .iter()
        .filter(|app| {
            app.extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
        .collect()
}

pub fn find_fuzzy_matches<'a>(
    applications: &'a [Application],
    search_term: &str,
) -> Vec<&'a Application> {
    let search_term = search_term.to_ascii_lowercase();
    applications
        .iter()
        .filter(|app| {
            app.name.to_ascii_lowercase().contains(&search_term)
                || app
                    .extensions
                    .iter()
                    .any(|extension| extension.to_ascii_lowercase().contains(&search_term))
                || app.handlers.iter().any(|handler| {
                    handler
                        .identifier
                        .to_ascii_lowercase()
                        .contains(&search_term)
                })
                || app.type_declarations.iter().any(|declaration| {
                    declaration
                        .identifier
                        .to_ascii_lowercase()
                        .contains(&search_term)
                        || declaration
                            .mime_types
                            .iter()
                            .chain(&declaration.extensions)
                            .chain(&declaration.conforms_to)
                            .any(|value| value.to_ascii_lowercase().contains(&search_term))
                })
        })
        .collect()
}

pub fn resolve_app<'a>(applications: &'a [Application], selector: &str) -> Vec<&'a Application> {
    let path_matches = applications
        .iter()
        .filter(|app| app.path.to_string_lossy() == selector)
        .collect::<Vec<_>>();
    if !path_matches.is_empty() {
        return path_matches;
    }

    let bundle_matches = applications
        .iter()
        .filter(|app| app.bundle_id.as_deref() == Some(selector))
        .collect::<Vec<_>>();
    if !bundle_matches.is_empty() {
        return bundle_matches;
    }

    applications
        .iter()
        .filter(|app| app.name.eq_ignore_ascii_case(selector))
        .collect()
}

pub fn normalize_extension(input: &str) -> Result<String> {
    let extension = input.trim().trim_start_matches('.').to_ascii_lowercase();
    if extension.is_empty() {
        bail!("please enter a valid file extension");
    }
    if !extension
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '_'))
    {
        bail!("file extensions may only contain letters, numbers, '+', '-' or '_'");
    }
    Ok(extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, path: &str, bundle_id: Option<&str>) -> Application {
        Application {
            name: name.to_owned(),
            path: PathBuf::from(path),
            bundle_id: bundle_id.map(str::to_owned),
            extensions: vec!["txt".to_owned()],
            handlers: Vec::new(),
            type_declarations: Vec::new(),
        }
    }

    #[test]
    fn resolves_by_path_bundle_id_and_name() {
        let applications = vec![
            app("Editor", "/Applications/Editor.app", Some("dev.editor")),
            app("Viewer", "/Applications/Viewer.app", Some("dev.viewer")),
        ];

        assert_eq!(
            resolve_app(&applications, "/Applications/Editor.app").len(),
            1
        );
        assert_eq!(resolve_app(&applications, "dev.viewer").len(), 1);
        assert_eq!(resolve_app(&applications, "editor").len(), 1);
        assert!(resolve_app(&applications, "missing").is_empty());
    }

    #[test]
    fn leaves_duplicate_names_ambiguous() {
        let applications = vec![
            app("Editor", "/Applications/Editor.app", Some("dev.editor")),
            app(
                "Editor",
                "/Applications/Alt/Editor.app",
                Some("dev.editor.alt"),
            ),
        ];
        assert_eq!(resolve_app(&applications, "Editor").len(), 2);
    }

    #[test]
    fn normalizes_and_validates_extensions() {
        assert_eq!(normalize_extension(" .TXT ").unwrap(), "txt");
        assert_eq!(normalize_extension("c++").unwrap(), "c++");
        assert!(normalize_extension("../../txt").is_err());
        assert!(normalize_extension("...").is_err());
    }

    #[test]
    fn finds_declared_handlers_with_role_implication() {
        let mut editor = app("Editor", "/Applications/Editor.app", Some("dev.editor"));
        editor.handlers = vec![DeclaredHandler {
            kind: crate::association::AssociationKind::Uti,
            identifier: "public.text".to_owned(),
            role: HandlerRole::Editor,
            source: crate::plist_parser::HandlerDeclarationSource::DocumentType,
        }];
        let applications = vec![editor];
        let viewer = AssociationTarget::new(
            crate::association::AssociationKind::Uti,
            "public.text",
            HandlerRole::Viewer,
        )
        .unwrap();
        let candidates = find_handler_candidates(&applications, &viewer);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].declarations[0].role, HandlerRole::Editor);
        let shell = AssociationTarget::new(
            crate::association::AssociationKind::Uti,
            "public.text",
            HandlerRole::Shell,
        )
        .unwrap();
        assert!(find_handler_candidates(&applications, &shell).is_empty());
    }
}
