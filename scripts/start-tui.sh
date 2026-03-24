#!/bin/bash
# 启动 Agent TUI 并连接到本地 Relay Server
cd "$(dirname "$0")/.."
export RELAY_TOKEN=test-token
export RELAY_PORT=3001
cargo run -p rust-agent-tui -- \
  --remote-control ws://localhost:3001 \
  --relay-token test-token \
  --relay-name 本地TUI
