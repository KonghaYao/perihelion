#!/bin/bash
set -e

# 加载 .env
set -a; source "$(dirname "$0")/.env"; set +a

# 确保日志目录存在
mkdir -p "$(dirname "$RUST_LOG_FILE")"

# 启动 TUI
cargo run -p rust-agent-tui -- "$@"
