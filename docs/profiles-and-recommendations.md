# Profiles and recommendations

Dutis includes four built-in profiles for common macOS workflows:

- `developer`: source code, scripts, structured data, and technical Markdown.
- `designer`: raster images, SVG assets, design-oriented PDFs.
- `media`: common audio and video formats.
- `minimal`: built-in macOS applications with no third-party dependency.

Profiles are ordered preferences, not hidden configuration. Inspect them
without scanning applications or requiring `duti`:

```bash
dutis profile list
dutis profile show developer
dutis profile show developer --json
```

## Local profile overlays

Dutis loads an optional version-one overlay from
`$DUTIS_STATE_DIR/profiles.toml`, or from the path in `DUTIS_PROFILE_FILE`.
Without either environment variable it checks
`~/Library/Application Support/dutis/profiles.toml`.
Use [`dutis.profiles.example.toml`](../dutis.profiles.example.toml) as a
starting point:

```toml
version = 1

[[profiles]]
name = "developer"
description = "Team development defaults."

[[profiles.associations]]
kind = "extension"
identifier = "md"
applications = ["com.example.TeamEditor", "dev.zed.Zed"]

[[profiles.associations]]
kind = "uti"
identifier = "public.source-code"
role = "editor"
applications = ["com.example.TeamEditor"]

[[profiles]]
name = "support-team"
description = "Support documents and web links."

[[profiles.associations]]
kind = "url_scheme"
identifier = "https"
applications = ["com.apple.Safari"]
```

An overlay with a built-in name extends that profile. Its applications are
placed before built-in candidates, with duplicate bundle IDs removed. Set
`replace_candidates = true` on an association to replace that target's
candidates, or `replace = true` on a profile to replace all its associations.
A new profile requires a description and at least one association.

Names, identifiers, roles, candidate lists, duplicate targets, file type, and
the 1 MiB file-size limit are validated before any profile is returned. An
explicitly configured missing file is an error. MCP clients use the server's
local overlay and cannot submit an overlay in a tool call.

## Generate a proposal

```bash
dutis recommend developer
dutis recommend developer --json
```

Recommendation scans installed applications and reads current handlers. It
does not change Launch Services. It loads the effective local policy before
ranking candidates. Built-in and overlaid profiles contribute association
targets; local policy can additionally contribute typed UTI, MIME, and
URL-scheme targets.
For each target, Dutis:

1. Honors a protected association as the required target.
2. Applies ordered target-specific preferences, then global preferences that
   match candidates in the selected profile.
3. Removes candidates excluded by kind, extension, or application allowlists.
4. Keeps the current eligible profile candidate when no installed policy
   preference takes precedence, minimizing unnecessary changes.
5. Otherwise selects the first eligible candidate with one installed path.
6. Requires typed candidates to explicitly declare the normalized handler and
   a compatible Launch Services role.
7. Marks a target `policy_blocked` when installed candidates exist but are
   excluded, or `unavailable` when no uniquely installed eligible candidate
   exists. Neither result enters the proposed configuration or plan.

Each result includes candidate source (`profile`, `global_preference`,
`extension_preference`, `handler_preference`, or `protected_policy`), policy
eligibility and reasons, installed paths, exact target declaration evidence,
selected target, current handler, and a human-readable explanation. Results
retain the legacy `extension` identifier and `declares_extension` fields, while
the additive `association` and `declares_target` fields carry typed semantics.
The complete response also includes proposed TOML, a deterministic
`AssociationPlan` and digest, and assessment against the same local policy.

Configure recommendation inputs in the local policy file:

```toml
[recommendations]
preferred_applications = ["com.microsoft.VSCode", "com.apple.TextEdit"]

[recommendations.extensions]
md = ["com.microsoft.VSCode"]
txt = ["com.apple.TextEdit", "com.microsoft.VSCode"]

[[recommendations.handlers]]
kind = "uti"
identifier = "public.plain-text"
role = "viewer"
applications = ["com.apple.TextEdit", "com.microsoft.VSCode"]

[[recommendations.handlers]]
kind = "mime"
identifier = "text/plain"
role = "editor"
applications = ["com.microsoft.VSCode"]

[[recommendations.handlers]]
kind = "url_scheme"
identifier = "vscode"
applications = ["com.microsoft.VSCode"]
```

`DUTIS_POLICY_FILE` selects the policy file. Otherwise Dutis uses
`$DUTIS_STATE_DIR/policy.toml` or the normal per-user state directory. Teams can
deploy that file with their existing device-management tooling; Dutis does not
fetch policy or accept recommendation overrides from a remote service.

Declared extension support is evidence rather than the only selector for
curated built-in profile candidates. Typed policy preferences are stricter:
the installed bundle metadata must explicitly declare the requested target and
compatible role. This prevents fleet policy from proposing a typed handler that
the local application does not advertise.

## Review and apply

A recommendation is never an approval. Save or copy the proposed TOML, inspect
it, and use the normal governed workflow:

```bash
dutis plan dutis.toml --json
dutis policy check dutis.toml --json
dutis apply dutis.toml --dry-run
dutis apply dutis.toml --plan-digest <reviewed-digest> --yes --requester <identity>
```

The apply command rebuilds the plan from current state, rejects a stale digest,
enforces policy, creates a safety snapshot, records an audit entry, and verifies
the resulting handlers.

## MCP tools

Read-only MCP servers advertise `dutis_profiles`, `dutis_profile`, and
`dutis_recommend`. `dutis_recommend` accepts `{ "profile": "developer" }` and
returns the same policy-aware evidence, proposed configuration, plan, and
policy assessment as the CLI. It uses the server's effective local policy and
does not accept caller-supplied policy. Agents must still use `dutis_diff` and
`dutis_policy_check`, obtain explicit approval, and provide the freshly
reviewed digest before calling a write tool.
