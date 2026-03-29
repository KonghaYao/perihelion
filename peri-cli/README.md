# peri-cli

CLI tool for installing and managing the Perihelion Rust Agent Framework.

## Quick Start

```bash
# Install perihelion
npx peri-cli

# Add to PATH (run after installation)
npx peri-cli add-env
```

## Commands

### `npx peri-cli install`

Install or update Perihelion to the latest version.

```bash
npx peri-cli                    # Install latest
npx peri-cli -v agent-v1.5      # Install specific version
```

### `npx peri-cli add-env`

Add `peri` to your PATH. Run this once after installation.

```bash
npx peri-cli add-env
source ~/.zshrc   # or ~/.bashrc
```

Then you can run directly:

```bash
peri
```

### `npx peri-cli list`

List available versions on GitHub.

```bash
npx peri-cli list
npx peri-cli ls
```

### `npx peri-cli update`

Update to the latest version.

```bash
npx peri-cli update
```

### `npx peri-cli uninstall`

Uninstall peri and clean up PATH.

```bash
npx peri-cli uninstall
```

## Installation Directory

```
~/.perihelion/
├── current-version.txt   # Current version marker
├── peri                  # Executable symlink
└── agent-v1.5/           # Version directory
    └── agent-tui         # Binary
```

## Supported Platforms

- macOS (x86_64, aarch64)
- Linux (x86_64, aarch64)
- Windows (x86_64)

## Development

```bash
cd peri-cli
bun install
bun run bin/peri-cli.js --help
```
