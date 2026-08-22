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

## Generate a proposal

```bash
dutis recommend developer
dutis recommend developer --json
```

Recommendation scans installed applications and reads current handlers. It
does not change Launch Services. For each extension, Dutis:

1. Keeps the current handler when it is a profile candidate with exactly one
   installed application, minimizing unnecessary changes.
2. Otherwise selects the first candidate with exactly one installed path.
3. Marks the extension unavailable when every candidate is missing or a bundle
   ID resolves to multiple paths. Unavailable entries are excluded from the
   proposed configuration and plan.

Each result includes the candidate order, installed paths, whether the app
declares support for the extension, the selected target, current handler, and
a human-readable reason. The complete response also includes proposed TOML, a
deterministic `AssociationPlan` and digest, and assessment against the current
local policy.

Declared extension support is evidence rather than the only selector. Some
applications accept formats that are not present in all versions of their
bundle metadata, so the ordered built-in profile remains the source of the
preference while the evidence stays visible for review.

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
returns the same evidence, proposed configuration, plan, and policy assessment
as the CLI. Agents must still use `dutis_diff` and `dutis_policy_check`, obtain
explicit approval, and provide the freshly reviewed digest before calling a
write tool.
