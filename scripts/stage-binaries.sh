#!/usr/bin/env bash
# Stage a built binary into multi-platform npm layout:
#   artifacts/bin/<platform>-<arch>/grok[.exe]
#
# Usage:
#   bash scripts/stage-binaries.sh win32-x64 /path/to/grok.exe
#   bash scripts/stage-binaries.sh darwin-arm64 /path/to/grok
#   bash scripts/stage-binaries.sh linux-x64 /path/to/grok
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKG="$(cd "$SCRIPT_DIR/.." && pwd)"
KEY="${1:-}"
SRC="${2:-}"
if [[ -z "$KEY" || -z "$SRC" || ! -f "$SRC" ]]; then
  echo "usage: $0 <platform-arch> <binary-path>" >&2
  echo "  e.g. $0 win32-x64 artifacts/grok-fork.exe" >&2
  echo "  keys: win32-x64 darwin-x64 darwin-arm64 linux-x64 linux-arm64" >&2
  exit 1
fi
case "$KEY" in
  win32-*) NAME=grok.exe ;;
  *) NAME=grok ;;
esac
DEST="$PKG/artifacts/bin/$KEY"
mkdir -p "$DEST"
cp -f "$SRC" "$DEST/$NAME"
chmod +x "$DEST/$NAME" 2>/dev/null || true
# Keep legacy Windows path for older tooling
if [[ "$KEY" == "win32-x64" ]]; then
  cp -f "$SRC" "$PKG/artifacts/grok-fork.exe"
fi
echo "staged $DEST/$NAME ($(wc -c < "$DEST/$NAME") bytes)"
