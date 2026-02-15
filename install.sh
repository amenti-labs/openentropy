#!/bin/sh
# OpenEntropy installer
# Usage: curl -sSf https://raw.githubusercontent.com/amenti-labs/openentropy/master/install.sh | sh
set -e

REPO="amenti-labs/openentropy"
BINARY="openentropy"

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Darwin) OS_TAG="apple-darwin" ;;
    Linux)  echo "Error: Pre-built Linux binaries are not yet available. Install from source:" \
                 "\n  cargo install --git https://github.com/${REPO} openentropy-cli"; exit 1 ;;
    *)      echo "Error: Unsupported OS: $OS"; exit 1 ;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    arm64|aarch64) ARCH_TAG="aarch64" ;;
    x86_64)        ARCH_TAG="x86_64" ;;
    *)             echo "Error: Unsupported architecture: $ARCH"; exit 1 ;;
esac

ASSET_NAME="${BINARY}-${ARCH_TAG}-${OS_TAG}"

# Get latest release tag
echo "Fetching latest release..."
LATEST=$(curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | sed 's/.*: "//;s/".*//')
if [ -z "$LATEST" ]; then
    echo "Error: Could not determine latest release"; exit 1
fi
echo "Latest release: $LATEST"

BASE_URL="https://github.com/${REPO}/releases/download/${LATEST}"

# Download binary and checksums
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${ASSET_NAME}..."
curl -sSfL "${BASE_URL}/${ASSET_NAME}" -o "${TMPDIR}/${BINARY}"
curl -sSfL "${BASE_URL}/checksums-sha256.txt" -o "${TMPDIR}/checksums-sha256.txt"

# Verify checksum
echo "Verifying checksum..."
cd "$TMPDIR"
EXPECTED=$(awk -v asset="${ASSET_NAME}" '$2 == asset {print $1; exit}' checksums-sha256.txt)
if [ -z "$EXPECTED" ]; then
    echo "Error: Could not find checksum for ${ASSET_NAME} in checksums-sha256.txt"
    exit 1
fi
ACTUAL=$(shasum -a 256 "${BINARY}" | awk '{print $1}')
if [ "$EXPECTED" != "$ACTUAL" ]; then
    echo "Error: Checksum mismatch!"
    echo "  Expected: $EXPECTED"
    echo "  Got:      $ACTUAL"
    exit 1
fi
echo "Checksum verified ✓"

# Install
chmod +x "${BINARY}"
if [ -d "$HOME/.cargo/bin" ]; then
    INSTALL_DIR="$HOME/.cargo/bin"
elif [ -w "/usr/local/bin" ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

cp "${BINARY}" "${INSTALL_DIR}/${BINARY}"

echo ""
echo "✅ OpenEntropy ${LATEST} installed to ${INSTALL_DIR}/${BINARY}"
echo ""
echo "Run 'openentropy scan' to discover entropy sources on your machine."

# Check PATH
case ":$PATH:" in
    *":${INSTALL_DIR}:"*) ;;
    *) echo "⚠️  ${INSTALL_DIR} is not in your PATH. Add it:" \
           "\n  export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
esac
