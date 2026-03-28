#!/bin/bash
# 启动 Relay Server（端口 8080，token=test-token）
cd "$(dirname "$0")/.."
export RELAY_TOKEN=test-token
export RELAY_PORT=8080
cargo run -p rust-relay-server
