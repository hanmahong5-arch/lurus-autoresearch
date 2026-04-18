#!/usr/bin/env sh
# resman — local-first experiment tracker for autonomous AI training agents.
# One-line install:  curl -fsSL https://raw.githubusercontent.com/kaizen-38/autoresearch/master/auto_research_task/resman/install.sh | sh
#
# Detects your OS+arch, downloads the matching prebuilt binary from the latest
# GitHub Release, and installs it to ~/.local/bin (or $RESMAN_INSTALL_DIR).

set -eu

REPO="kaizen-38/autoresearch"
INSTALL_DIR="${RESMAN_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${RESMAN_VERSION:-latest}"

die() { printf 'error: %s\n' "$1" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# --- detect target triple ---------------------------------------------------
uname_s=$(uname -s)
uname_m=$(uname -m)

case "$uname_s" in
    Linux)  os=unknown-linux-gnu ;;
    Darwin) os=apple-darwin ;;
    *)      die "unsupported OS: $uname_s (supported: Linux, macOS). On Windows, use: cargo install --path ." ;;
esac

case "$uname_m" in
    x86_64|amd64) arch=x86_64 ;;
    arm64|aarch64) arch=aarch64 ;;
    *) die "unsupported architecture: $uname_m" ;;
esac

target="${arch}-${os}"

# --- resolve release tag ----------------------------------------------------
# GitHub release tags for resman are prefixed `resman-v<semver>` so the repo
# can host multiple crates under separate release series. Pass RESMAN_VERSION
# as either "resman-v0.4.0" (full tag) or "v0.4.0" (shorthand, auto-prefixed).
if [ "$VERSION" = "latest" ]; then
    have curl || die "curl is required"
    # Walk releases newest-first, pick the first whose tag begins with resman-v.
    tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30" \
          | sed -n 's/.*"tag_name":[[:space:]]*"\(resman-v[^"]*\)".*/\1/p' \
          | head -n1)
    [ -n "$tag" ] || die "could not resolve latest resman-v* release tag for $REPO"
else
    case "$VERSION" in
        resman-v*) tag="$VERSION" ;;
        v*)        tag="resman-$VERSION" ;;
        *)         tag="resman-v$VERSION" ;;
    esac
fi

# Asset name matches .github/workflows/resman.yml packaging step:
#   name="resman-${GITHUB_REF_NAME#resman-}-${target}"  → resman-v0.4.0-x86_64-unknown-linux-gnu.tar.gz
version="${tag#resman-}"
asset="resman-${version}-${target}.tar.gz"
url="https://github.com/$REPO/releases/download/${tag}/${asset}"

printf 'resman %s · %s\n' "$tag" "$target"
printf 'downloading %s\n' "$url"

# --- download + extract -----------------------------------------------------
tmpdir=$(mktemp -d 2>/dev/null || mktemp -d -t resman)
trap 'rm -rf "$tmpdir"' EXIT

have curl || die "curl is required"
have tar  || die "tar is required"

curl -fsSL "$url" -o "$tmpdir/$asset" \
    || die "download failed. see https://github.com/$REPO/releases for manual install."

tar -xzf "$tmpdir/$asset" -C "$tmpdir" \
    || die "extract failed"

# Accept either resman or resman.exe at tarball root or one level deep.
bin_src=""
for cand in "$tmpdir/resman" "$tmpdir/resman.exe" "$tmpdir"/*/resman "$tmpdir"/*/resman.exe; do
    if [ -f "$cand" ]; then bin_src="$cand"; break; fi
done
[ -n "$bin_src" ] || die "could not find resman binary inside $asset"

# --- install ----------------------------------------------------------------
mkdir -p "$INSTALL_DIR"
install_path="$INSTALL_DIR/resman"
cp "$bin_src" "$install_path"
chmod +x "$install_path"

printf 'installed: %s\n' "$install_path"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        printf '\nadd this to your shell rc to use `resman` directly:\n'
        printf '    export PATH="%s:$PATH"\n' "$INSTALL_DIR"
        ;;
esac

printf '\nquickstart:\n'
printf '    resman init\n'
printf '    resman add -t demo -c $(git rev-parse --short HEAD) -v 0.99 -s keep -d "baseline"\n'
printf '    resman best -f value\n'
