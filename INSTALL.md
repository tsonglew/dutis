# Installation Guide for Dutis

## Quick Start (Recommended)

### Via Homebrew

```bash
brew install tsonglew/dutis/dutis
```

That's it! The application will automatically install the required `duti` dependency if needed.

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
- Rust 1.70 or later
- Homebrew (for automatic duti installation)

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

1. Check if `duti` is available
2. Automatically install `duti` via Homebrew if needed
3. Start scanning your system applications
4. Enter interactive mode

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

The application should automatically install `duti` via Homebrew. If this fails:

1. Ensure Homebrew is installed:

   ```bash
   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
   ```

2. Manually install duti:

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
- **Dependencies**: Homebrew (for automatic duti installation)

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
