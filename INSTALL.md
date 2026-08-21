# Installation Guide for Dutis

## Quick Start (Recommended)

### Via Homebrew

```bash
brew install tsonglew/tap/dutis
```

The formula installs the universal `dutis` binary and its `duti` dependency.

## Alternative Installation Methods

### From Pre-built Binary

1. Download the latest release from [GitHub Releases](https://github.com/tsonglew/dutis/releases)
2. Extract the binary
3. Move to a directory in your PATH:

   ```bash
   sudo mv dutis /usr/local/bin/
   # or
   sudo mv dutis /opt/homebrew/bin/
   ```

### From Source

#### Prerequisites

- macOS 10.14 or later
- Rust 1.88 or later
- `duti` when changing default applications

#### Build Steps

```bash
# Clone the repository
git clone https://github.com/tsonglew/dutis.git
cd dutis

# Build the project
cargo build --release

# Install globally
cargo install --path .

# Or run directly
./target/release/dutis
```

## Post-Installation

### Verify Installation

```bash
dutis --help
```

### First Run

```bash
dutis
```

The application will:

1. Scan your system applications
2. Read their declared filename extensions
3. Enter interactive mode
4. Check for `duti` only when you choose to change a default

## Updating

### Via Homebrew

```bash
brew update && brew upgrade dutis
```

### From Source

```bash
cd dutis
git pull origin main
cargo build --release
cargo install --path .
```

## Uninstalling

### Via Homebrew

```bash
brew uninstall dutis
```

### From Source

```bash
cargo uninstall dutis
```

## Troubleshooting

### "duti not found" Error

Install `duti` with Homebrew:

```bash
brew install duti
```

### Permission Issues

If you encounter permission errors:

1. Check Homebrew permissions:

   ```bash
   brew doctor
   ```

2. Ensure proper ownership:

   ```bash
   sudo chown -R $(whoami) /opt/homebrew
   ```

### Rust Not Found

If Rust is not installed:

1. Install via Homebrew:

   ```bash
   brew install rust
   ```

2. Or install via rustup:

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

## System Requirements

- **Operating System**: macOS 10.14 (Mojave) or later
- **Architecture**: Intel (x86_64) or Apple Silicon (arm64)
- **Memory**: 512MB RAM minimum
- **Storage**: 50MB free space
- **Dependencies**: `duti` is required only for changing default applications

## Support

If you encounter any issues:

1. Check the [GitHub Issues](https://github.com/tsonglew/dutis/issues) page
2. Create a new issue with:
   - macOS version
   - Error message
   - Steps to reproduce
   - System information

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.
