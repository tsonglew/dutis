# Installing Dutis via Homebrew

## Quick Install

```bash
brew install tsonglew/tap/dutis
```

The formula also installs the bundled agent skill under:

```text
$(brew --prefix dutis)/share/dutis/skills/dutis
```

The policy example is installed at
`$(brew --prefix dutis)/share/dutis/dutis.policy.example.toml`.

To make it discoverable by Codex, link it into your personal skills directory:

```bash
mkdir -p ~/.codex/skills
ln -s "$(brew --prefix dutis)/share/dutis/skills/dutis" ~/.codex/skills/dutis
```

## What This Does

When you install `dutis` via Homebrew:

1. **Automatic Dependencies**: Homebrew installs the required `duti` runtime dependency
2. **Binary Installation**: Downloads and installs a universal binary for Intel and Apple Silicon Macs
3. **Path Setup**: Adds `dutis` to your system PATH
4. **Updates**: Easy updates with `brew upgrade dutis`

## Manual Tap Setup

If you prefer to add the tap first:

```bash
# Add the tap
brew tap tsonglew/tap

# Install dutis
brew install dutis
```

## Updating

```bash
# Update all Homebrew packages including dutis
brew update && brew upgrade

# Update only dutis
brew upgrade dutis
```

## Uninstalling

```bash
brew uninstall dutis
```

## Troubleshooting

### If you get a "No available formula" error

1. Make sure you have the latest Homebrew:

   ```bash
   brew update
   ```

2. Try installing directly:

   ```bash
   brew install tsonglew/tap/dutis
   ```

### If you get permission errors

1. Make sure Homebrew is properly installed:

   ```bash
   brew doctor
   ```

2. Check your Homebrew permissions:

   ```bash
   ls -la /opt/homebrew/bin/brew
   ```

## Requirements

- macOS 10.14 or later
- Homebrew installed
- Internet connection for first installation

## Benefits of Homebrew Installation

- **Automatic Updates**: Easy to keep up to date
- **Dependency Management**: Handles all required dependencies
- **System Integration**: Properly integrated with macOS
- **Rollback**: Easy to revert to previous versions if needed
- **Clean Uninstall**: Removes all associated files when uninstalling
