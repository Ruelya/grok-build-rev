#!/usr/bin/env bash
# Build release binary from a patched (or fork) grok-build tree.
# Does NOT install/replace ~/.grok/bin/grok — only produces target/release artifact.
#
# Usage:
#   ./scripts/build-fork.sh [path-to-grok-build-src]
#   ./scripts/build-fork.sh --check-only [path]
set -euo pipefail

# Resolve package root before any `cd` — `$0` is relative to the caller's CWD.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
PKG_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CHECK_ONLY=0
SRC=""
for a in "$@"; do
  case "$a" in
    --check-only) CHECK_ONLY=1 ;;
    *) SRC="$a" ;;
  esac
done
SRC="${SRC:-$PWD}"
SRC="$(cd "$SRC" && pwd)"

export PATH="${HOME}/.cargo/bin:${USERPROFILE:-}/.cargo/bin:${PATH}"

if [[ ! -f "$SRC/Cargo.toml" ]]; then
  echo "error: not a cargo workspace: $SRC" >&2
  exit 1
fi

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing $1 — $2" >&2
    exit 1
  fi
}

need cargo "install Rust via rustup"
need rustc "install Rust via rustup"

if ! command -v dotslash >/dev/null 2>&1; then
  echo "dotslash not found; installing with cargo install dotslash …"
  cargo install dotslash --locked
fi

# Windows: repo bin/protoc is a DotSlash script without windows-x86_64.
# Prefer PROTOC env, then a local win64 unpack under D:/Download/protoc-win, then PATH.
if [[ -z "${PROTOC:-}" ]]; then
  for cand in \
    "D:/Download/protoc-win/bin/protoc.exe" \
    "$HOME/.local/protoc/bin/protoc.exe" \
    "/c/Users/pc/.local/protoc/bin/protoc.exe"
  do
    if [[ -x "$cand" || -f "$cand" ]]; then
      export PROTOC="$cand"
      break
    fi
  done
fi
if [[ -n "${PROTOC:-}" ]]; then
  echo "PROTOC=$PROTOC"
  # Also put dir on PATH so child processes find protoc.exe by name
  export PATH="$(dirname "$PROTOC"):$PATH"
elif ! command -v protoc >/dev/null 2>&1; then
  echo "warn: no protoc — Windows builds need a real protoc.exe (not bin/protoc DotSlash)" >&2
  echo "      download: https://github.com/protocolbuffers/protobuf/releases" >&2
fi

cd "$SRC"
echo "tree: $SRC"
echo "HEAD: $(git rev-parse --short HEAD 2>/dev/null || echo n/a)"
echo "toolchain: $(rustc --version)"

if [[ "$CHECK_ONLY" -eq 1 ]]; then
  cargo check -p xai-grok-pager-bin
  exit 0
fi

echo "building: cargo build -p xai-grok-pager-bin --release"
cargo build -p xai-grok-pager-bin --release

ART=""
for cand in \
  "$SRC/target/release/xai-grok-pager.exe" \
  "$SRC/target/release/xai-grok-pager" \
  "$SRC/target/release/grok.exe" \
  "$SRC/target/release/grok"
do
  if [[ -f "$cand" ]]; then ART="$cand"; break; fi
done

if [[ -z "$ART" ]]; then
  echo "error: release binary not found under target/release/" >&2
  ls -la "$SRC/target/release/" 2>/dev/null | head -40 || true
  exit 1
fi

OUT_DIR="${GROK_FORK_OUT:-$SRC/dist-fork}"
mkdir -p "$OUT_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
BASE_NAME="grok-fork-${STAMP}"
if [[ "$ART" == *.exe ]]; then
  DEST="$OUT_DIR/${BASE_NAME}.exe"
else
  DEST="$OUT_DIR/${BASE_NAME}"
fi
cp -f "$ART" "$DEST"
# also stable name for scripts
if [[ "$ART" == *.exe ]]; then
  cp -f "$ART" "$OUT_DIR/grok-fork.exe"
else
  cp -f "$ART" "$OUT_DIR/grok-fork"
fi

GIT_HEAD="$(git -C "$SRC" rev-parse HEAD 2>/dev/null || echo n/a)"
GIT_SHORT="$(git -C "$SRC" rev-parse --short HEAD 2>/dev/null || echo n/a)"
VER_LINE="n/a"
if [[ -x "$DEST" || -f "$DEST" ]]; then
  VER_LINE="$("$DEST" --version 2>/dev/null | head -1 || true)"
fi

cat > "$OUT_DIR/BUILD_INFO.txt" <<EOF
built_at=$(date -Iseconds 2>/dev/null || date)
brand=ruelya
source=$SRC
git_head=$GIT_HEAD
version=$VER_LINE
artifact=$DEST
cargo_pkg=xai-grok-pager-bin
binary_name=xai-grok-pager
features=openai_responses-type-level-loose,toolset-style-main+subagent,auto_pck,recap,Lonetrail-semantic-band,opencode-themes,open-subagents
EOF

# Publish into install-patch package when this script lives there
if [[ -d "$PKG_ROOT/artifacts" ]]; then
  if [[ "$ART" == *.exe ]]; then
    cp -f "$ART" "$PKG_ROOT/artifacts/grok-fork.exe"
  else
    cp -f "$ART" "$PKG_ROOT/artifacts/grok-fork"
  fi
  cp -f "$OUT_DIR/BUILD_INFO.txt" "$PKG_ROOT/artifacts/BUILD_INFO.txt"
  echo "package:   $PKG_ROOT/artifacts/grok-fork$( [[ "$ART" == *.exe ]] && echo .exe )"
fi

echo
echo "OK artifact: $DEST"
echo "also:       $OUT_DIR/grok-fork$( [[ "$ART" == *.exe ]] && echo .exe )"
echo "version:    $VER_LINE"
echo
echo "Install onto already-installed grok (after quitting all grok):"
echo "  bash $PKG_ROOT/scripts/patch-installed.sh apply --dry-run"
echo "  bash $PKG_ROOT/scripts/patch-installed.sh apply"
echo "Replace notes: $PKG_ROOT/REPLACE_LOCAL.md"
