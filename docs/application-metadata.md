# Application metadata

Dutis v2.14 reads typed Launch Services declarations from installed application
bundles. This supports evidence-backed discovery before selecting or changing a
default handler.

## Query registered handlers

Use the typed query command for extensions, UTIs, MIME types, and URL schemes:

```bash
dutis handler query extension md
dutis handler query uti public.plain-text --role viewer
dutis handler query mime text/plain --role editor --json
dutis handler query url-scheme https --json
```

The MCP equivalent is `dutis_handler_query`, with `kind`, `identifier`, and an
optional `role`. Identifiers use the same normalization and validation as
planning and mutation commands.

Each result contains a compact application identity and only the declarations
that matched the query:

```json
{
  "name": "TextEdit",
  "path": "/System/Applications/TextEdit.app",
  "bundle_id": "com.apple.TextEdit",
  "declarations": [
    {
      "kind": "uti",
      "identifier": "public.plain-text",
      "role": "editor",
      "source": "document_type"
    }
  ]
}
```

## Evidence sources

Dutis treats these Info.plist records as handler registrations:

- `CFBundleDocumentTypes` with `CFBundleTypeExtensions`,
  `LSItemContentTypes`, or `CFBundleTypeMIMETypes`;
- `CFBundleTypeRole` as `viewer`, `editor`, `shell`, or `all` when omitted;
- `CFBundleURLTypes` with `CFBundleURLSchemes`, always using role `all`.

An explicit `CFBundleTypeRole = None` or an unknown role is not reported as a
handler capability. Invalid and wildcard identifiers are ignored rather than
making the whole application unreadable.

For discovery, an `editor` declaration also satisfies a `viewer` query. An
`all` query returns every matching declaration. Editor and shell capabilities
do not imply one another.

## Type definitions

`UTExportedTypeDeclarations` and `UTImportedTypeDeclarations` define or import
types; they do not by themselves prove that the application opens those types.
Dutis therefore exposes them separately as `type_declarations`, including:

- normalized `UTTypeIdentifier`;
- whether it is `exported` or `imported`;
- normalized `UTTypeConformsTo` identifiers;
- `public.filename-extension` tags;
- `public.mime-type` tags.

The legacy `extensions` array remains available for compatibility and still
contains extension tags from both document registrations and type definitions.
Use `handlers` or `handler query` when the distinction matters.

## Full catalog

`dutis list --json` and MCP `dutis_list` expose `handlers` and
`type_declarations` alongside the existing name, path, bundle ID, and extension
fields. Empty typed arrays are omitted to keep output compact. Metadata from
one malformed application is counted in `metadata_failures` without hiding
other applications.
