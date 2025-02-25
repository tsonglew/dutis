# Dutis

[![CI](https://github.com/tsonglew/dutis/actions/workflows/ci.yml/badge.svg)](https://github.com/tsonglew/dutis/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/tsonglew/dutis)](https://github.com/tsonglew/dutis/blob/master/LICENSE)
[![Crates.io](https://img.shields.io/crates/v/dutis)](https://crates.io/crates/dutis)
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/tsonglew/dutis)](https://github.com/tsonglew/dutis/releases)

A command-line tool to manage default applications for file types on macOS. It provides an interactive interface to set default applications for individual file extensions or groups of related file types (like video, audio, images, etc.).

> ⚠️ **Note**: This tool is designed specifically for macOS and will not work on other operating systems.

> ⚠️ **Warning**: This tool relies on deprecated macOS CoreServices APIs (deprecated since macOS 10.4–12.0). While it currently works, it may become unstable or stop working in future macOS versions. Apple has not provided direct replacements for these APIs, making this the only available approach for programmatically managing file type associations.

## Features

- 🎯 Set default applications for individual file extensions
- 👥 Batch set default applications for groups of file types (video, audio, image, etc.)
- 🔍 Interactive selection of applications
- 🎨 Color-coded output for better visibility
- ⚡ Fast and efficient Rust implementation
- 🔄 Supports common file type groups out of the box

## Installation

### Building from Source

```shell
git clone https://github.com/tsonglew/dutis.git
cd dutis
cargo build --release
```

## Usage

### Basic Usage

```shell
# Set default application for a single file extension
sudo dutis <file-extension>

# Example: Set default application for .mp4 files
sudo dutis mp4
```

### Group Mode

```shell
# Set default application for a group of file types
sudo dutis --group <group-name>

# Example: Set default application for all video files
sudo dutis --group video
```

> ⚠️ **Note**: `sudo` is required because changing default applications requires system-level permissions.

### Available Groups

The following file type groups are supported:

- `video`: Common video formats (mp4, avi, mkv, etc.)
- `audio`: Audio formats (mp3, wav, aac, etc.)
- `image`: Image formats (jpg, png, gif, etc.)
- `code`: Programming and markup files (py, js, rs, etc.)
- `archive`: Archive formats (zip, tar, gz, etc.)

You can view the full list of supported extensions in the `config/groups.yaml` file.

## Configuration

Dutis uses a YAML configuration file to define file type groups. The default configuration is located at `config/groups.yaml`. You can modify this file to add or remove file extensions from groups.

Example group configuration:

```yaml
groups:
  video:
    - mp4
    - avi
    - mkv
    # ...
  audio:
    - mp3
    - wav
    - aac
    # ...
```

## Requirements

- macOS operating system
- Rust 1.56 or later (for building from source)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Stargazers over time

[![Stargazers over time](https://starchart.cc/tsonglew/dutis.svg?variant=adaptive)](https://starchart.cc/tsonglew/dutis)
