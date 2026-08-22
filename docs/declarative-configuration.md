# Declarative configuration

Dutis v2.6 adds a versioned TOML configuration and a reviewable
`plan -> apply -> verify` workflow for scripts and AI agents.

## Schema version 1

```toml
version = 1

[associations]
md = "com.microsoft.VSCode"
json = "/Applications/Visual Studio Code.app"
txt = "TextEdit"
```

Association keys are filename extensions. A leading dot and uppercase letters
are accepted and normalized. Each value must resolve to exactly one installed
application using, in priority order:

1. Exact application path.
2. Exact bundle identifier.
3. Case-insensitive, unambiguous application name.

Unknown fields, unsupported schema versions, invalid extensions, empty
selectors, and duplicate normalized extensions are rejected.

## Review and apply

Create a plan without changing the system:

```bash
dutis plan dutis.toml
dutis plan dutis.toml --json
```

The plan records current and desired bundle identifiers, resolved application
paths, changes, unchanged entries, unresolved selectors, and a SHA-256 digest.
The digest covers the normalized desired state and the current state.

Show only differences and unresolved entries:

```bash
dutis diff dutis.toml
```

Preview the apply path without making changes:

```bash
dutis apply dutis.toml --dry-run --json
```

Apply a reviewed plan by copying its digest:

```bash
dutis apply dutis.toml \
  --plan-digest <digest-from-plan> \
  --requester <human-or-agent-id> \
  --yes \
  --json
```

Dutis rebuilds the plan immediately before applying it. If defaults, installed
applications, or configuration changed since review, the digest differs and
the command exits with code `7` without making changes.

Each changed association is applied and read back for verification. An error
for one association does not hide other results: Dutis continues, returns every
per-entry result, and exits with code `8` when any item fails. Entries already
in the desired state are skipped, so reapplying a converged configuration is a
no-op.

Before the first mutation, Dutis evaluates the plan against the effective local
policy and creates a persistent pending audit record. Policy denial uses exit
code `9` and no association is changed. See
[policy and mutation audit](policy-and-audit.md).

## Versioning and migration

- `version` is the configuration schema version and is currently `1`.
- `schema_version` in plan JSON is currently `1`.
- `api_version` in the CLI JSON envelope remains `1`.
- Unknown configuration fields are rejected so misspellings cannot silently
  change policy.
- A future incompatible schema will use a new configuration version and include
  explicit migration documentation. Dutis never silently upgrades a file.

## Exit codes

| Code | Meaning |
| ---: | --- |
| `0` | Success, including a converged no-op |
| `2` | Invalid command or configuration |
| `3` | Unresolved application selector |
| `5` | `duti` is unavailable |
| `6` | State inspection or operation failed |
| `7` | Reviewed plan is stale |
| `8` | One or more associations failed to apply or verify |
| `9` | Local mutation policy denied the plan or approval |
