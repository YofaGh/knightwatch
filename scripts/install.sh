#!/usr/bin/env bash
# Installer for the knightwatch family of tools.
# Usage:
#   curl -LsSf https://raw.githubusercontent.com/YofaGh/knightwatch/main/scripts/install.sh | sh
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

# The binary name matches the package name for every crate in this repo today.
# If that ever diverges for a future crate, set it explicitly here instead.
BIN_NAME="$PACKAGE"

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

# Resolve "latest" to the newest release tag for THIS package specifically.
# GitHub's /releases/latest shortcut is repo-wide (whichever crate released
# most recently), so it can't be used once multiple crates release independently.
resolve_latest_tag() {
  curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=100" \
    | grep -o "\"tag_name\": *\"${PACKAGE}/[^\"]*\"" \
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
    */*) tag="$VERSION" ;;                # already a full tag, e.g. "knightwatch-cli/1.0.0"
    *)   tag="${PACKAGE}/${VERSION}" ;;    # just a version number, e.g. "1.0.0"
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