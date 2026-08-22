# Snapshots and rollback

Dutis v2.7 stores local, versioned snapshots before declarative changes and
uses the same plan, apply, and verification pipeline for rollback.

## Storage

Snapshots are JSON files stored at:

```text
~/Library/Application Support/dutis/snapshots/<snapshot-id>.json
```

Set `DUTIS_STATE_DIR` to use another state directory, which is useful for CI,
isolated agent runs, or backups. Files are written atomically with owner-only
permissions on Unix. They contain extension associations, bundle identifiers,
application paths reported by `duti`, timestamps, and plan digests. They do not
contain tokens or credentials.

## Create and inspect

Capture every extension declared by installed applications:

```bash
dutis snapshot create
```

Limit capture to extensions in a declarative configuration:

```bash
dutis snapshot create --config dutis.toml --json
```

List snapshots in newest-first order:

```bash
dutis history
dutis history --json
```

Every declarative `apply` that has changes creates a `before_apply` safety
snapshot before invoking `duti`. A rollback with changes creates a
`before_rollback` snapshot, allowing the rollback itself to be reversed.
Converged no-op plans do not create redundant snapshots.

## Roll back

Always review the rollback plan first:

```bash
dutis rollback <snapshot-id> --dry-run
dutis rollback <snapshot-id> --dry-run --json
```

Apply and verify it explicitly:

```bash
dutis rollback <snapshot-id> --yes --json
```

Rollback resolves every recorded bundle identifier against currently installed
applications, reads current state, builds a deterministic plan, applies each
change, and verifies it. Missing or ambiguous applications block the entire
rollback before mutation. Runtime failures retain the pre-rollback safety
snapshot and return per-entry results.

## Known safe limitation

`duti` can set an association but does not provide a safe command to remove one.
If a snapshot records that an extension had no default and it now has one,
Dutis marks that entry unresolved and refuses the whole rollback. It never
claims to restore an absent association or performs a broad Launch Services
reset. The snapshot remains available for inspection and future tooling.

## Snapshot schema

Snapshot `schema_version` is currently `1`. Unknown versions are rejected.
Snapshot identifiers accept only ASCII letters, numbers, and hyphens, preventing
path traversal. Corrupt snapshot files cause history or rollback to fail loudly
instead of being silently ignored.
