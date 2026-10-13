#!/usr/bin/env bash
# Builds the CLI and the MCP server and copies them to src-tauri/binaries as
# <name>-<target-triple>[.exe], where Tauri's bundle.externalBin expects them.
# Usage: scripts/stage-sidecars.sh [target-triple]   (default: the host triple)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
[ -n "$target" ] || { echo "cannot determine the target triple" >&2; exit 1; }

[[ "$target" =~ ^[A-Za-z0-9_.-]+$ ]] || { echo "invalid target triple: $target" >&2; exit 1; }

case "$target" in *windows*) ext=".exe" ;; *) ext="" ;; esac

cd "$root"
cargo build --release --locked --target "$target" -p agentlens-cli -p agentlens-mcp

# Honor CARGO_TARGET_DIR and build.target-dir.
if [ -n "${CARGO_TARGET_DIR:-}" ]; then
  target_dir="$CARGO_TARGET_DIR"
else
  target_dir="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
fi
[ -n "$target_dir" ] || { echo "cannot determine the cargo target directory" >&2; exit 1; }

out="$root/src-tauri/binaries"
mkdir -p "$out"
for name in agentlens agentlens-mcp; do
  cp "$target_dir/$target/release/$name$ext" "$out/$name-$target$ext"
  echo "staged $out/$name-$target$ext"
done
