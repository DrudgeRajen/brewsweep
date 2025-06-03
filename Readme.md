# BrewSweep

A fast, terminal-based tool for managing your Homebrew packages by tracking their usage and helping you identify packages that can be safely removed. Built with Rust and ratatui for a responsive TUI experience.

![BrewSweep](https://img.shields.io/badge/TUI-Rust-orange?style=for-the-badge&logo=rust)
![License](https://img.shields.io/badge/License-MIT-blue?style=for-the-badge)
![Platform](https://img.shields.io/badge/Platform-macOS-lightgrey?style=for-the-badge)

## What is it?

**BrewSweep** scans your installed Homebrew packages (both formulas and casks) and displays them sorted by last access time. This makes it easy to identify:

- **Never-used packages** - Installed but never accessed
- **Rarely-used packages** - Haven't been used in months or years  
- **Recently-used packages** - Actively used packages to keep

The tool provides a clean, interactive interface for viewing package details and safely removing unused packages to free up disk space. **Sweep away the clutter** and keep only what you actually use!

## Features

### 📊 **Smart Package Analysis**
- **Real-time scanning** of all Homebrew formulas and casks
- **Last access time tracking** using filesystem metadata
- **Automatic sorting** by usage (least recently used first)
- **Detailed package information** including installation paths

### 🎯 **Easy Package Management**
- **Interactive table view** with keyboard navigation
- **Package details screen** showing comprehensive information
- **Safe deletion workflow** with confirmation dialogs
- **Real-time uninstall output** showing brew command progress

### 🖥️ **Terminal UI**
- **Responsive interface** built with ratatui
- **Color-coded display** with multiple themes
- **Keyboard shortcuts** for efficient navigation
- **Progress indicators** for all operations

### ⚡ **Performance**
- **Fast scanning** using parallel processing
- **Non-blocking UI** - responsive during operations
- **Memory efficient** handling of large package lists
- **Background operations** for deletions

## Installation

### Prerequisites
- **Rust** (1.70 or later) - [Install Rust](https://rustup.rs/)
- **Homebrew** installed and in PATH - [Install Homebrew](https://brew.sh/)
- **macOS** (primary platform) 

### Using Cargo (Recommended)
```bash
cargo install brewsweep
```

### From Source
```bash
# Clone the repository
git clone https://github.com/DrudgeRajen/brewsweep.git
cd brewsweep

# Build and install
cargo build --release

# Run the application
./target/release/brewsweep
```

### Using Cargo from Git
```bash
# Install directly from git
cargo install --git https://github.com/DrudgeRajen/brewsweep.git
```

## Usage

### Basic Workflow

1. **Start the application**
   ```bash
   brewsweep
   ```

2. **Scan packages**
   - Press `Space` to start scanning your Homebrew installation
   - Watch real-time progress as packages are discovered

3. **Browse packages**
   - Use `↑`/`↓` arrow keys to navigate the package list
   - Packages are automatically sorted with least-used first

4. **View details**
   - Press `Enter` on any package to see detailed information
   - View last access time, type, and installation path

5. **Delete packages**
   - Press `d` to delete a selected package
   - Confirm with `y` or cancel with `n`
   - Watch real-time output from the `brew uninstall` command

### Keyboard Controls

#### Main Table
| Key | Action |
|-----|--------|
| `Space` | Start package scan |
| `↑`/`↓` | Navigate up/down |
| `←`/`→` | Navigate left/right |
| `Enter` | View package details |
| `d` | Delete selected package |
| `r` | Refresh (re-scan packages) |
| `Shift + →` | Next color theme |
| `Shift + ←` | Previous color theme |
| `Esc` | Quit application |

#### Package Details
| Key | Action |
|-----|--------|
| `Enter`/`Space` | Back to table |
| `d` | Delete this package |
| `Esc` | Quit application |

#### Deletion Confirmation
| Key | Action |
|-----|--------|
| `y`/`Enter` | Confirm deletion |
| `n`/`Space` | Cancel deletion |
| `Esc` | Quit application |


### Package Information Display

The tool displays packages with the following information:

- **Package Name** - The Homebrew package identifier
- **Type** - Formula (command-line tool) or Cask (GUI application)
- **Last Accessed** - Human-readable time since last use:
  - "Never accessed" - Package never used
  - "2 hours ago" - Recently used
  - "3 months ago" - Moderately old
  - "1 year ago" - Very old, candidate for removal
- **Path** - Installation location on your system

### Sorting Logic

Packages are automatically sorted by usage to prioritize cleanup candidates:

1. **Never accessed packages** (top of list)
   - Easiest to identify for removal
   - Safe to delete if you don't recognize them

2. **Oldest accessed packages** 
   - Haven't been used in months/years
   - Good candidates for cleanup

3. **Recently accessed packages** (bottom of list)
   - Actively used, probably should keep
   - Latest access times

## Development

### Building from Source
```bash
git clone https://github.com/DrudgeRajen/brewsweep.git
cd brewsweep
cargo build --release
```

### Dependencies
This project uses the following Rust crates:
- `ratatui` - Terminal user interface
- `crossterm` - Cross-platform terminal handling
- `color-eyre` - Error handling and reporting
- `unicode-width` - Text width calculation


## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

### Development Setup
```bash
git clone https://github.com/DrudgeRajen/brewsweep.git
cd brewsweep
cargo build
cargo run
```

### Running Tests
```bash
cargo test
```

### Code Style
This project uses standard Rust formatting:
```bash
cargo fmt
cargo clippy
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [ratatui](https://github.com/ratatui-org/ratatui) for the excellent terminal UI framework
- Uses [crossterm](https://github.com/crossterm-rs/crossterm) for cross-platform terminal support
- Error handling powered by [color-eyre](https://github.com/eyre-rs/color-eyre)
- Inspired by the need to manage ever-growing Homebrew installations efficiently

## Support

If you find this tool useful, please consider:
- ⭐ Starring the repository
- 🐛 Reporting bugs and issues
- 💡 Suggesting new features
- 🤝 Contributing code improvements

---

**Happy Homebrew sweeping! 🧹✨**

*Keep your system lean and your packages meaningful.*
