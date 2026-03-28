# Perihelion CLI Installer

Node.js CLI tool for installing, updating, and managing the Perihelion Rust Agent Framework.

## Installation

```bash
cd cli
npm install
npm link
```

Or use npx directly:

```bash
npx perihelion install
```

## Commands

### `perihelion install` or `peri install`

Install Perihelion to the latest version.

**Options:**
- `-v, --version <version>` - Install a specific version

**Examples:**
```bash
perihelion install              # Install latest version
perihelion install -v v0.1.0    # Install specific version
peri install -v v0.2.0           # Use short alias
```

### `perihelion list` or `peri list`

List the top 5 versions published on GitHub.

**Examples:**
```bash
perihelion list
peri ls
```

Example output:
```
📋 Available versions:

  1. agent-v0.2.0 (latest) (current)
     Name: Perihelion v0.2.0
     Published: 3/28/2026
     URL: https://github.com/konghayao/perihelion/releases/tag/agent-v0.2.0
     Binary: agent-tui-macos-aarch64 (15.2 MB)

  2. agent-v0.1.0
     Name: Perihelion v0.1.0
     Published: 3/20/2026
     URL: https://github.com/konghayao/perihelion/releases/tag/agent-v0.1.0
     Binary: agent-tui-macos-aarch64 (14.8 MB)
```

### `perihelion update` or `peri update`

Update Perihelion to the latest version.

**Examples:**
```bash
perihelion update
peri update
```

## GitHub Release Naming

The project uses `agent-v*` tag format for releases, for example:
- `agent-v0.1.0` - First version
- `agent-v0.2.0` - Second version

Binary file naming:
- `agent-tui-linux-x86_64` - Linux x64
- `agent-tui-macos-x86_64` - macOS x64 (Intel)
- `agent-tui-macos-aarch64` - macOS ARM64 (Apple Silicon)
- `agent-tui-windows-x86_64.exe` - Windows x64

## Installation Location

Binaries are installed to the `~/.perihelion/` directory:

```
~/.perihelion/
├── current-version.txt          # Current version marker
├── bin                          # Symlink to active version
├── agent-v0.2.0/                # Version directory
│   └── agent-tui               # Binary file
└── agent-v0.1.0/                # Historical version directory
    └── agent-tui
```

## Platform Support

Supported platforms:
- macOS (x86_64, aarch64)
- Linux (x86_64, aarch64)
- Windows (x86_64)

## Development

```bash
cd cli
npm install                     # Install dependencies
npm link                        # Create global link
perihelion list                # Test commands
```

## Uninstall

```bash
npm unlink                      # Remove global link
rm -rf ~/.perihelion            # Remove installation files
```
