#!/usr/bin/env bash
# Installer for the knightwatch family of tools.
# Usage:
#   curl -LsSf https://raw.githubusercontent.com/YofaGh/knightwatch/master/scripts/install.sh | sh
#   curl -LsSf .../install.sh | sh -s -- --package knightwatch-cli
#   curl -LsSf .../install.sh | sh -s -- --version 1.0.17
#   curl -LsSf .../install.sh | sh -s -- --package knightwatch-cli --version 1.0.0
set -euo pipefail

REPO="YofaGh/knightwatch"
PACKAGE="knightwatch"
VERSION="latest"
INSTALL_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"

while [ $# -gt 0 ]; do
  case "$1" in
    --package) PACKAGE="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --install-dir) INSTALL_DIR="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

# --- Package/tag/binary naming ------------------------------------------
#
# Three names are involved for each crate in this repo, and they are NOT
# guaranteed to be the same string:
#
#   1. CRATE NAME   - what the crate is called in Cargo.toml. This is also
#                      the prefix used in git tags, e.g. "knightwatch-cli/1.0.1".
#                      Release resolution (resolve_latest_tag below) always
#                      keys off the crate name, because that's what the tags
#                      use.
#   2. BIN_NAME     - the actual binary produced by the crate, and the
#                      prefix of the release asset filenames, e.g.
#                      "kwctl-x86_64-unknown-linux-gnu.tar.gz".
#   3. --package    - what the *user* types on the command line. We accept
#                      either the crate name or the binary name here for
#                      convenience, since users often only know the binary
#                      they run day to day.
#
# When a crate's binary name matches its crate name, no entry below is
# needed (see the "*" fallthrough). Only add a case here when a crate's
# binary name diverges from its crate name - as knightwatch-cli/kwctl does.
#
# To add a new crate whose binary name differs from its crate name:
#   1. Add a line mapping the crate name -> its binary name.
#   2. Add a line mapping the binary name -> the same binary name, so users
#      can pass either one via --package.
resolve_bin_name() {
  case "$1" in
    knightwatch-cli) echo "kwctl" ;;
    kwctl) echo "kwctl" ;;
    *) echo "$1" ;;
  esac
}

# The crate/tag name is whatever --package resolves the binary name's
# "owning" crate to be. Since tags are keyed by crate name, and a user might
# pass either the crate name or the binary name, normalize --package back to
# the crate name for tag lookups.
resolve_crate_name() {
  case "$1" in
    kwctl) echo "knightwatch-cli" ;;
    *) echo "$1" ;;
  esac
}

CRATE_NAME="$(resolve_crate_name "$PACKAGE")"
BIN_NAME="$(resolve_bin_name "$PACKAGE")"
# -------------------------------------------------------------------------

uname_os="$(uname -s)"
uname_arch="$(uname -m)"

case "$uname_os" in
  Linux) os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "Unsupported OS: $uname_os" >&2; exit 1 ;;
esac

case "$uname_arch" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "Unsupported architecture: $uname_arch" >&2; exit 1 ;;
esac

target="${arch}-${os}"
archive="${BIN_NAME}-${target}.tar.gz"

# Resolve "latest" to the newest release tag for THIS crate specifically.
# GitHub's /releases/latest shortcut is repo-wide (whichever crate released
# most recently), so it can't be used once multiple crates release independently.
# NOTE: tags are keyed by CRATE_NAME, not BIN_NAME - see naming note above.
resolve_latest_tag() {
  curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=100" \
    | grep -o "\"tag_name\": *\"${CRATE_NAME}/[^\"]*\"" \
    | head -n1 \
    | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
}

if [ "$VERSION" = "latest" ]; then
  tag="$(resolve_latest_tag)"
  if [ -z "$tag" ]; then
    echo "Could not find any release for package '${PACKAGE}' in ${REPO}" >&2
    exit 1
  fi
else
  case "$VERSION" in
    */*) tag="$VERSION" ;;                     # already a full tag, e.g. "knightwatch-cli/1.0.0"
    *)   tag="${CRATE_NAME}/${VERSION}" ;;      # just a version number, e.g. "1.0.0"
  esac
fi

url="https://github.com/${REPO}/releases/download/${tag}/${archive}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

echo "Downloading ${url}"
curl --proto '=https' --tlsv1.2 -LsSf "$url" -o "${tmpdir}/${archive}"
tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"

stage_dir="${tmpdir}/${BIN_NAME}-${target}"
mkdir -p "$INSTALL_DIR"
install -m 755 "${stage_dir}/${BIN_NAME}" "${INSTALL_DIR}/${BIN_NAME}"

echo "Installed ${BIN_NAME} (${tag}) to ${INSTALL_DIR}/${BIN_NAME}"

case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *) echo "Note: ${INSTALL_DIR} is not on your PATH. Add it to your shell profile." ;;
esac