# Dutis - macOS Application File Extension Manager

A comprehensive Rust application for viewing file extensions supported by macOS applications and setting default applications for file types.

[![Release](https://github.com/tsonglew/dutis/actions/workflows/release.yml/badge.svg)](https://github.com/tsonglew/dutis/actions/workflows/release.yml)

## Features

- 🔍 **Scan System Applications**: Automatically discovers all installed applications on macOS
- 📱 **File Extension Analysis**: Shows which file extensions each application supports
- 🎯 **Interactive Query Mode**: Search for applications that support specific file types
- ⚙️ **Default App Setting**: Set default applications for file types using the `duti` command
- 🚀 **UTI Detection**: Intelligent UTI (Uniform Type Identifier) detection with retry mechanisms
- 📊 **Categorized Display**: File extensions are organized by category
- 🔧 **Auto-dependency Management**: Automatically installs `duti` via Homebrew if not available

## Installation

### Prerequisites

- macOS 10.14 or later
- Homebrew (for automatic duti installation)

### Via Homebrew (Recommended)

```bash
# Install dutis directly from Homebrew
brew install tsonglew/dutis/dutis
```

### Automatic duti Installation

The application will automatically check for `duti` on startup and install it via Homebrew if it's not available. No manual installation is required!

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

## How It Works

### Application Scanning

1. **System Directories**: Scans `/Applications`, `/System/Applications`, and `~/Applications`
2. **Info.plist Parsing**: Reads each application's `Info.plist` file to extract supported file extensions
3. **UTI Mapping**: Maps file extensions to their corresponding UTI (Uniform Type Identifier)

### Default App Setting

1. **Bundle ID Detection**: Uses `mdls` command to get the application's Bundle Identifier
2. **UTI Detection**: Creates temporary files with appropriate content to detect UTI
3. **Retry Mechanism**: Implements intelligent retry logic for UTI detection
4. **duti Integration**: Uses the `duti` command to set system-wide default applications

## Technical Details

### Architecture

- **Modular Design**: Separated into logical modules (`app_scanner`, `plist_parser`, `platform`)
- **Cross-Platform Ready**: Platform-specific implementations with trait abstractions
- **Error Handling**: Comprehensive error handling using `anyhow`
- **Async Ready**: Designed to be easily extended with async operations

### Dependencies

- **anyhow**: Error handling and propagation
- **colored**: Terminal output formatting and colors
- **walkdir**: Directory traversal

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
