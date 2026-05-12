# peri-cli

CLI tool for installing and managing the Peri Rust Agent Framework.

## Quick Start

```bash
# Install perihelion
npx peri-cli

# Add to PATH (run after installation)
npx peri-cli add-env
```

## Commands

### `npx peri-cli install [package]`

Install a package. Supports package name, full version tag, or no argument (defaults to latest agent).

```bash
npx peri-cli install              # Install latest agent
npx peri-cli install agent        # Install latest agent
npx peri-cli install agent-v1.17  # Install specific agent version
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

### `npx peri-cli update [package]`

Update a package to the latest version.

```bash
npx peri-cli update              # Update agent to latest
npx peri-cli update agent        # Same as above
```

### `npx peri-cli uninstall`

Uninstall peri and clean up PATH.

```bash
npx peri-cli uninstall
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `GITHUB_PROXY` | GitHub download proxy URL, replaces `https://github.com` prefix in download URLs |
| `PERI_GITHUB_PROXY` | Same as above, takes precedence over `GITHUB_PROXY` |

### GitHub Proxy

If you have trouble downloading from GitHub (e.g. connection timeout), set `GITHUB_PROXY` to a GitHub mirror/proxy service. The value replaces the `https://github.com` prefix in download URLs.

```bash
GITHUB_PROXY=<your-proxy-url>/https://github.com npx peri-cli install agent
```

You can also set it persistently in your shell profile:

```bash
export GITHUB_PROXY=<your-proxy-url>/https://github.com
```

## Installation Directory

```
~/.perihelion/
├── current-version.txt   # Current version marker
├── peri                  # Executable symlink
└── agent-v1.17/          # Agent version directory
    └── agent             # Binary
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
