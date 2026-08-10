#!/usr/bin/env bash
# Local release for @ruelya/grok-build (no GitHub Actions build).
#
# Builds the *current host* binary, stages it as a Release asset name, then:
#   1) Creates/updates GitHub Release vX.Y.Z with any assets under dist-release/
#   2) Publishes thin npm package (downloads binaries from that Release on install)
#
# Usage (from repo root):
#   export PATH="/d/Download/protoc-win/bin:$PATH"   # Windows: real protoc
#   bash npm/scripts/release-local.sh              # auto version (patch if 1.0.0 exists)
#   bash npm/scripts/release-local.sh 1.0.1        # force version
#   SKIP_BUILD=1 bash npm/scripts/release-local.sh # only package already-built assets
#
# Platforms this machine can produce:
#   Windows → grok-win32-x64.exe
#   Linux   → grok-linux-x64
#   macOS arm64 → grok-darwin-arm64
#
# Collect multi-platform assets into dist-release/ from other machines, then
# SKIP_BUILD=1 to publish all three at once.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NPM_DIR="$REPO_ROOT/npm"
DIST="$REPO_ROOT/dist-release"
PKG_NAME="@ruelya/grok-build"
REPO="${GITHUB_REPOSITORY:-Ruelya/grok-build-rev}"

cd "$REPO_ROOT"
mkdir -p "$DIST"

FORCE_VER="${1:-}"
SKIP_BUILD="${SKIP_BUILD:-0}"

detect_asset() {
  case "$(uname -s 2>/dev/null || echo Windows)" in
    MINGW*|MSYS*|CYGWIN*|Windows_NT|Windows)
      echo "win32-x64|xai-grok-pager.exe|grok-win32-x64.exe"
      ;;
    Darwin)
      if [[ "$(uname -m)" == "arm64" ]]; then
        echo "darwin-arm64|xai-grok-pager|grok-darwin-arm64"
      else
        echo "UNSUPPORTED darwin-x64 (Intel macOS not shipped)" >&2
        return 1
      fi
      ;;
    Linux)
      echo "linux-x64|xai-grok-pager|grok-linux-x64"
      ;;
    *)
      echo "unknown host" >&2
      return 1
      ;;
  esac
}

if [[ "$SKIP_BUILD" != "1" ]]; then
  IFS='|' read -r KEY SRC_NAME ASSET <<<"$(detect_asset)"
  echo "==> build $KEY ($ASSET)"
  if ! command -v protoc >/dev/null 2>&1 && [[ -z "${PROTOC:-}" ]]; then
    if [[ -x "/d/Download/protoc-win/bin/protoc.exe" ]]; then
      export PATH="/d/Download/protoc-win/bin:$PATH"
    fi
  fi
  cargo build -p xai-grok-pager-bin --release
  SRC="target/release/$SRC_NAME"
  if [[ ! -f "$SRC" ]]; then
    echo "error: missing $SRC" >&2
    exit 1
  fi
  cp -f "$SRC" "$DIST/$ASSET"
  chmod +x "$DIST/$ASSET" 2>/dev/null || true
  echo "    staged $DIST/$ASSET ($(wc -c < "$DIST/$ASSET") bytes)"
  "$DIST/$ASSET" --version || true
else
  echo "==> SKIP_BUILD=1 — using existing $DIST/*"
fi

ASSETS=( "$DIST"/grok-win32-x64.exe "$DIST"/grok-linux-x64 "$DIST"/grok-darwin-arm64 )
FOUND=()
for a in "${ASSETS[@]}"; do
  [[ -f "$a" ]] && FOUND+=("$a")
done
if [[ ${#FOUND[@]} -eq 0 ]]; then
  echo "error: no release assets in $DIST" >&2
  echo "  expected any of: grok-win32-x64.exe grok-linux-x64 grok-darwin-arm64" >&2
  exit 1
fi
echo "==> assets to publish (${#FOUND[@]}):"
printf '    %s\n' "${FOUND[@]}"

# Version
cd "$NPM_DIR"
if [[ -n "$FORCE_VER" ]]; then
  VER="$FORCE_VER"
else
  LATEST=$(npm view "$PKG_NAME" version 2>/dev/null || true)
  if [[ -z "$LATEST" ]]; then
    VER=$(node -p "require('./package.json').version")
  else
    IFS=. read -r MA MI PA <<<"$LATEST"
    PA=${PA%%[^0-9]*}
    VER="${MA}.${MI}.$((PA+1))"
  fi
fi
BASE="https://github.com/${REPO}/releases/download/v${VER}"
echo "==> version $VER"
echo "    release base $BASE"

VER="$VER" BASE="$BASE" node -e '
  const fs=require("fs");
  const p=JSON.parse(fs.readFileSync("package.json","utf8"));
  p.name=process.env.PKG || "@ruelya/grok-build";
  p.version=process.env.VER;
  p.forkReleaseBase=process.env.BASE;
  p.publishConfig={access:"public",registry:"https://registry.npmjs.org/"};
  p.files=(p.files||[]).filter(f => !String(f).includes("artifacts/bin"));
  fs.writeFileSync("package.json", JSON.stringify(p,null,2)+"\n");
  console.log(p.name, p.version);
' 

mkdir -p artifacts
{
  echo "package: $PKG_NAME"
  echo "version: $VER"
  echo "built: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host: $(uname -a 2>/dev/null || echo windows)"
  ls -la "$DIST"
} > artifacts/BUILD_INFO.txt

TAG="v${VER}"
NOTES="Install: npm install -g ${PKG_NAME}@${VER} | Assets: $(printf '%s ' "${FOUND[@]##*/}") | local build"

echo "==> GitHub Release $TAG"
if gh release view "$TAG" -R "$REPO" >/dev/null 2>&1; then
  gh release upload "$TAG" "${FOUND[@]}" -R "$REPO" --clobber
else
  gh release create "$TAG" "${FOUND[@]}" -R "$REPO" \
    --title "$PKG_NAME $VER" \
    --notes "$NOTES"
fi

echo "==> npm pack (thin)"
rm -rf artifacts/bin artifacts/cache 2>/dev/null || true
npm pack

if [[ "${SKIP_NPM_PUBLISH:-0}" == "1" ]]; then
  echo "SKIP_NPM_PUBLISH=1 — not calling npm publish"
  echo "    tarball ready under npm/"
  exit 0
fi

echo "==> npm publish (needs npm login / automation token on this machine)"
if npm publish --access public; then
  echo "OK $PKG_NAME@$VER"
else
  echo "npm publish failed — Release assets are already on GitHub ($TAG)." >&2
  echo "  Fix auth then: cd npm && npm publish --access public" >&2
  exit 1
fi
