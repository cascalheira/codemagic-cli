#!/usr/bin/env sh
# install.sh — installs the latest codemagic-cli release on macOS and Linux.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.sh | sh
#
# Environment variables (all optional):
#   INSTALL_DIR   — directory to install the binary (default: /usr/local/bin)
#   VERSION       — specific release tag to install (default: latest)

set -e

REPO="cascalheira/codemagic-cli"
BINARY="codemagic-cli"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# ── Detect OS ──────────────────────────────────────────────────────────────
OS="$(uname -s)"
case "$OS" in
  Darwin) OS_NAME="macos" ;;
  Linux)  OS_NAME="linux" ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

# ── Detect architecture ────────────────────────────────────────────────────
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64 | amd64)  ARCH_NAME="x86_64"  ;;
  arm64  | aarch64) ARCH_NAME="aarch64" ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

ASSET_NAME="${BINARY}-${OS_NAME}-${ARCH_NAME}.tar.gz"

# ── Resolve version ─────────────────────────────────────────────────────────
if [ -z "$VERSION" ]; then
  echo "Fetching latest release info from GitHub…"
  # Follow redirects; extract the tag from the final URL
  LATEST_URL="https://github.com/${REPO}/releases/latest"
  VERSION="$(curl -fsSL -o /dev/null -w '%{url_effective}' "$LATEST_URL" | sed 's|.*/||')"
  if [ -z "$VERSION" ]; then
    echo "Could not determine the latest release version." >&2
    exit 1
  fi
fi

echo "Installing ${BINARY} ${VERSION} for ${OS_NAME}/${ARCH_NAME}…"

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}"
CHECKSUM_URL="${DOWNLOAD_URL}.sha256"

# ── Download ─────────────────────────────────────────────────────────────────
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading ${ASSET_NAME}…"
curl -fsSL "$DOWNLOAD_URL" -o "${TMP_DIR}/${ASSET_NAME}"

# ── Verify checksum ───────────────────────────────────────────────────────────
echo "Verifying checksum…"
curl -fsSL "$CHECKSUM_URL" -o "${TMP_DIR}/${ASSET_NAME}.sha256"

# The .sha256 file contains "<hash>  <filename>"; cd into TMP_DIR so the
# relative path in the checksum file resolves correctly.
(
  cd "$TMP_DIR"
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum -c "${ASSET_NAME}.sha256"
  elif command -v shasum > /dev/null 2>&1; then
    shasum -a 256 -c "${ASSET_NAME}.sha256"
  else
    echo "Warning: no sha256 tool found — skipping checksum verification." >&2
  fi
)

# ── Extract & install ─────────────────────────────────────────────────────────
echo "Extracting…"
tar -xzf "${TMP_DIR}/${ASSET_NAME}" -C "$TMP_DIR"

# Ensure the install directory exists
if [ ! -d "$INSTALL_DIR" ]; then
  echo "Creating ${INSTALL_DIR}…"
  mkdir -p "$INSTALL_DIR" 2>/dev/null || sudo mkdir -p "$INSTALL_DIR"
fi

# Copy binary; fall back to sudo if the directory isn't writable
if [ -w "$INSTALL_DIR" ]; then
  mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
else
  echo "Elevated permissions required to write to ${INSTALL_DIR}…"
  sudo mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
fi

chmod +x "${INSTALL_DIR}/${BINARY}" 2>/dev/null || sudo chmod +x "${INSTALL_DIR}/${BINARY}"

echo ""
echo "✓  ${BINARY} ${VERSION} installed to ${INSTALL_DIR}/${BINARY}"
echo ""

# ── Verify installation ───────────────────────────────────────────────────────
if command -v "$BINARY" > /dev/null 2>&1; then
  echo "Run '${BINARY}' to get started."
else
  echo "Note: ${INSTALL_DIR} is not in your PATH."
  echo "Add it with:  export PATH=\"${INSTALL_DIR}:\$PATH\""
fi
