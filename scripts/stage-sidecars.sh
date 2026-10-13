#!/usr/bin/env bash
# Builds the CLI and the MCP server and copies them to src-tauri/binaries as
# <name>-<target-triple>[.exe], where Tauri's bundle.externalBin expects them.
# Usage: scripts/stage-sidecars.sh [target-triple]   (default: the host triple)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
[ -n "$target" ] || { echo "cannot determine the target triple" >&2; exit 1; }

case "$target" in *windows*) ext=".exe" ;; *) ext="" ;; esac

cd "$root"
cargo build --release --locked --target "$target" -p agentlens-cli -p agentlens-mcp

out="$root/src-tauri/binaries"
mkdir -p "$out"
for name in agentlens agentlens-mcp; do
  cp "target/$target/release/$name$ext" "$out/$name-$target$ext"
  echo "staged $out/$name-$target$ext"
done
