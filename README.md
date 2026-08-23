# Dutis - macOS Default Application Manager

A Rust application for inspecting and safely managing macOS default handlers for filename extensions, UTIs, MIME types, and URL schemes.

**Website:** [tsonglew.github.io/dutis](https://tsonglew.github.io/dutis/)

[![CI](https://github.com/tsonglew/dutis/actions/workflows/ci.yml/badge.svg)](https://github.com/tsonglew/dutis/actions/workflows/ci.yml)
[![Release](https://github.com/tsonglew/dutis/actions/workflows/release.yml/badge.svg)](https://github.com/tsonglew/dutis/actions/workflows/release.yml)

## Features

- 🔍 **Scan System Applications**: Automatically discovers all installed applications on macOS
- 📱 **Application Capability Discovery**: Reads declared extensions, UTIs, MIME types, URL schemes, and roles
- 🎯 **Interactive Query Mode**: Search for applications that support specific file types
- ⚙️ **Default App Setting**: Set default applications for file types using the `duti` command
- 🧭 **Accurate App Selection**: Preserves full application paths, including nested and duplicate names
- 🧩 **Modern Metadata Support**: Reads legacy document types and modern UTI declarations
- ✅ **Verified Updates**: Verifies the selected default application after applying it
- 🤖 **Agent-ready CLI**: Stable JSON output, dry runs, deterministic selectors, and explicit exit codes
- 📋 **Declarative Configuration**: Plan, diff, apply, and verify a versioned TOML policy
- ↩️ **Snapshots and Rollback**: Persist pre-change state and restore it through the verified plan pipeline
- 🔌 **Local MCP Server**: Give agents read-only discovery and planning tools with separately gated writes
- 🛡️ **Policy and Audit**: Enforce local allowlists and approvals with durable, verified mutation records
- 💡 **Explainable Profiles**: Generate evidence-backed developer, designer, media, or minimal proposals without changing the system
- 🏢 **Fleet-Aware Recommendations**: Apply local allowlists, protected targets, and ordered team preferences before proposing applications
- 👀 **Drift Monitoring**: Detect association changes continuously, notify through macOS, and optionally remediate through snapshots and policy
- 🔗 **Typed Associations**: Manage extensions, UTIs, MIME types, URL schemes, and Launch Services roles through one verified pipeline
- 📡 **Event Sinks**: Stream versioned drift and mutation lifecycle events to private JSONL logs or trusted local commands
- 🌐 **HTTPS Event Adapter**: Forward events with environment-only credentials, bounded retries, and idempotency headers
- 📦 **Durable Event Replay**: Queue failed command deliveries locally and replay them without repeating the original mutation
- 🍎 **Native Role Inspection**: Read separate viewer, editor, and shell defaults directly from macOS Launch Services

## Installation

### Prerequisites

- macOS 10.14 or later
- `duti` (`brew install duti`) when changing default applications; the Homebrew formula installs it automatically

### Via Homebrew (Recommended)

```bash
brew install tsonglew/tap/dutis
```

The tap installs universal `dutis` and `dutis-event-http` binaries for Apple
Silicon and Intel Macs, together with the `duti` runtime dependency.

### Install duti

Browsing applications does not require `duti`. To change a default application, install it with:

```bash
brew install duti
```

### Manual Installation

### Build from Source

```bash
# Clone the repository
git clone https://github.com/tsonglew/dutis.git
cd dutis

# Build the project
cargo build --release

# Run the application
cargo run
```

### Install from binary

```bash
cargo install --path .
```

### Interactive Mode

The application starts in interactive mode where you can:

1. **View All Applications**: See a comprehensive list of all applications and their supported file extensions
2. **Search by Extension**: Enter a file extension (e.g., `txt`, `pdf`, `py`) to find supporting applications
3. **Set Default Apps**: Choose an application to set as the default for a specific file type
4. **Debug Information**: Access detailed scanning information

### Command Line Mode

Use explicit commands from shell scripts or AI agents. A leading dot on an
extension is optional.

```bash
# Inspect installed applications and supported handlers
dutis list
dutis query md
dutis get .md

# Inspect typed Launch Services handlers
dutis handler query uti public.plain-text --role viewer
dutis handler query mime text/plain --role editor
dutis handler query url-scheme https
dutis handler get uti public.plain-text --role viewer
dutis handler get mime text/plain --role editor
dutis handler get url-scheme https
dutis handler defaults extension txt --json

# Emit a versioned JSON response
dutis query json --json

# Preview a change without mutating the system
dutis set md com.microsoft.VSCode --dry-run --json

# Apply and verify a change; --yes is required for non-interactive writes
dutis set md com.microsoft.VSCode --yes

# Preview a typed handler change (URL schemes use the implicit `all` role)
dutis handler set uti public.plain-text com.apple.TextEdit --role viewer --dry-run
dutis handler set url-scheme https com.apple.Safari --yes

# Check local readiness
dutis doctor --json
```

Manage several associations as one reviewed, idempotent plan:

```bash
cp dutis.example.toml dutis.toml
dutis plan dutis.toml --json
dutis diff dutis.toml
dutis apply dutis.toml --dry-run
dutis apply dutis.toml --plan-digest <reviewed-digest> --yes
```

`apply` rebuilds the plan immediately before changing the system and rejects a
stale digest. Every change is verified, unchanged entries are skipped, and
partial failures include a result for every association. See the
[declarative configuration guide](docs/declarative-configuration.md) for the
schema and safety contract.

Typed queries return compact application candidates with the exact matching
Info.plist declarations. Full `list --json` output also includes each
application's registered handlers and imported/exported type definitions. See
[application metadata](docs/application-metadata.md) for evidence and role
matching rules.

Role-specific `handler get` queries and verification use native macOS Launch
Services reads. `handler defaults` returns the complete role matrix. See
[native Launch Services reads](docs/native-launch-services.md).

Create, inspect, and restore local snapshots:

```bash
dutis snapshot create --config dutis.toml
dutis history
dutis rollback <snapshot-id> --dry-run
dutis rollback <snapshot-id> --yes
```

Real declarative applies and rollbacks automatically store a safety snapshot
before the first mutation. See [snapshots and rollback](docs/snapshots-and-rollback.md)
for storage, recovery behavior, and the safe limitation around removing an
association.

Run the local MCP server in its default read-only mode:

```bash
dutis mcp
```

Mutation tools are registered only with `--allow-writes` and require both a
fresh plan digest and the server-side `DUTIS_MCP_APPROVAL_TOKEN`. See the
[MCP server guide](docs/mcp-server.md) for client configuration, tool schemas,
and the audit contract.

Inspect policy decisions and persistent mutation records:

```bash
dutis policy show --json
dutis policy check dutis.toml --json
dutis audit --json
```

Explore built-in profiles and generate a read-only recommendation:

```bash
dutis profile list
dutis profile show developer --json
dutis recommend developer --json
```

Recommendations use the effective local policy before selecting a target.
Teams can deploy preferences without a remote control plane:

```toml
[recommendations]
preferred_applications = ["com.microsoft.VSCode"]

[recommendations.extensions]
md = ["com.microsoft.VSCode", "com.apple.TextEdit"]
```

Results show each candidate's source, policy eligibility, installed paths,
declared extension support, the proposed TOML, a deterministic plan digest, and
the effective policy assessment. They never change system associations.
Review [profiles and recommendations](docs/profiles-and-recommendations.md) for
selection rules and the safe path from a proposal to an approved apply.

Check a declared configuration once or monitor it continuously:

```bash
dutis watch dutis.toml --once --json
dutis watch dutis.toml --interval-seconds 60 --notify
```

Install an optional per-user LaunchAgent that keeps the monitor running:

```bash
dutis launch-agent install dutis.toml --interval-seconds 300 --notify
dutis launch-agent status
```

Monitoring is read-only by default. Automatic remediation requires
`--remediate --yes --requester <identity>` and always passes through policy,
audit, safety snapshot, apply, and verification. See
[drift detection](docs/drift-detection.md).

Send drift and mutation lifecycle events to automation without parsing human
output:

```bash
dutis --event-log ./dutis-events.jsonl watch dutis.toml --once --json
dutis --event-command /absolute/path/to/event-handler apply dutis.toml \
  --plan-digest <reviewed-digest> --requester codex --yes
```

Event options are global and can appear before or after a subcommand. The same
settings can be provided through `DUTIS_EVENT_LOG`, `DUTIS_EVENT_COMMAND`, and
`DUTIS_EVENT_OUTBOX`. See [event sinks](docs/event-sinks.md) for the schema,
command contract, LaunchAgent behavior, and delivery guarantees.

To forward events over HTTPS without placing credentials in Dutis arguments,
configuration, or audit records, use the bundled
[`dutis-event-http`](docs/http-event-adapter.md) command:

```bash
export DUTIS_HTTP_ENDPOINT='https://events.example.com/hooks/dutis'
export DUTIS_HTTP_BEARER_TOKEN='replace-with-a-scoped-token'
export DUTIS_EVENT_COMMAND="$(command -v dutis-event-http)"
dutis-event-http --check --json

# Inspect and replay failed command deliveries
dutis events pending --json
dutis events replay --limit 100 --json
dutis events archive --max-attempts 5 --older-than-days 30 --json
dutis events archive --max-attempts 5 --older-than-days 30 --yes
dutis events dead-letters --json
dutis events purge --older-than-days 90 --json
dutis events purge --older-than-days 90 --yes
```

Failed event-command deliveries are stored automatically under the Dutis state
directory. Override that location with `--event-outbox` or
`DUTIS_EVENT_OUTBOX`. Replay keeps the original event ID so remote consumers
can deduplicate retries. See [durable event replay](docs/event-replay.md).
Archive and purge commands are previews unless `--yes` is present, so pending
deliveries are never removed by an implicit retention policy.

All write paths enforce the same local policy before mutation and record the
requester, reviewed plan, result, and verification. See the
[policy and audit guide](docs/policy-and-audit.md). A reusable agent workflow is
included at [`skills/dutis/SKILL.md`](skills/dutis/SKILL.md).
Homebrew installs it under `$(brew --prefix dutis)/share/dutis/skills/dutis`.

Applications can be selected by exact bundle ID, exact application path, or an
unambiguous application name. JSON responses use API version `1`. Exit codes are
`0` for success, `2` for usage errors, `3` for no match, `4` for ambiguous
selectors, `5` for an unavailable dependency, and `6` for operation failure.
Declarative apply uses `7` for a stale plan. Apply and event replay use `8` for
partial failure.
Policy denial uses exit code `9`.

The product and engineering sequence for declarative configuration, rollback,
MCP, agent policies, profiles, drift detection, and event delivery is documented in the
[Agent Roadmap](docs/agent-roadmap.md).

## How It Works

### Application Scanning

1. **System Directories**: Scans `/Applications`, `/System/Applications`, and `~/Applications`
2. **Info.plist Parsing**: Reads document types, URL types, roles, extensions, UTIs, and MIME types
3. **Modern Metadata**: Keeps handler registrations separate from exported and imported UTI definitions

### Default App Setting

1. **Bundle ID Detection**: Reads the selected application's bundle identifier
2. **duti Integration**: Sets extension, UTI, MIME, and URL-scheme handlers with the requested Launch Services role
3. **Verification**: Reads the resulting association back before reporting success

## Technical Details

### Architecture

- **Modular Design**: Separates application scanning and plist parsing from the interactive flow
- **macOS Native**: Works with application bundles and Launch Services through `duti`
- **Error Handling**: Comprehensive error handling using `anyhow`

### Dependencies

- **anyhow**: Error handling and propagation
- **colored**: Terminal output formatting and colors
- **plist**: Native XML and binary plist parsing
- **clap**: Command parsing and generated help
- **serde / serde_json**: Versioned machine-readable output
- **toml**: Strict declarative configuration parsing
- **sha2**: Deterministic reviewed-plan digests
- **time**: Portable RFC 3339 snapshot timestamps

## Releases

After a version bump is merged into `master`, CI validates the commit, creates the matching version tag, publishes a universal macOS binary and checksum, then updates the `tsonglew/homebrew-tap` repository. Maintainer setup and release instructions are documented in [docs/releasing.md](docs/releasing.md).

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Community

### Stargazers

[![Stargazers over time](https://starchart.cc/tsonglew/dutis.svg)](https://starchart.cc/tsonglew/dutis)

### Contributors

[![Contributors](https://contrib.rocks/image?repo=tsonglew/dutis)](https://github.com/tsonglew/dutis/graphs/contributors)

## License

This project is licensed under the MIT License.

## Support

If you encounter any issues or have questions, please create an issue on GitHub.
