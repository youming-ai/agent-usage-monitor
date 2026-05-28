#!/bin/sh
set -e

REPO="youming-ai/llm-usage-monitor"
BINARY_NAME="usage-monitor"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Darwin) OS_NAME="darwin" ;;
    Linux)  OS_NAME="linux" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    arm64|aarch64) ARCH_NAME="arm64" ;;
    x86_64)        ARCH_NAME="amd64" ;;
    *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

ASSET_NAME="usage-monitor-${OS_NAME}-${ARCH_NAME}.tar.gz"

# Get latest release URL
if command -v curl >/dev/null 2>&1; then
    DOWNLOAD_URL=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep "browser_download_url.*${ASSET_NAME}" \
        | head -1 \
        | cut -d '"' -f 4)
elif command -v wget >/dev/null 2>&1; then
    DOWNLOAD_URL=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep "browser_download_url.*${ASSET_NAME}" \
        | head -1 \
        | cut -d '"' -f 4)
else
    echo "Error: curl or wget is required"
    exit 1
fi

if [ -z "$DOWNLOAD_URL" ]; then
    echo "Error: Could not find release asset for ${OS_NAME}/${ARCH_NAME}"
    echo "Check https://github.com/${REPO}/releases for available downloads"
    exit 1
fi

echo "Downloading ${BINARY_NAME} from ${DOWNLOAD_URL}..."

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o "$TMPDIR/${ASSET_NAME}"
else
    wget -qO "$TMPDIR/${ASSET_NAME}" "$DOWNLOAD_URL"
fi

echo "Extracting..."
tar xzf "$TMPDIR/${ASSET_NAME}" -C "$TMPDIR"

# Try to install to INSTALL_DIR, fall back to ~/.local/bin
if [ -w "$INSTALL_DIR" ] 2>/dev/null; then
    mv "$TMPDIR/${BINARY_NAME}" "$INSTALL_DIR/${BINARY_NAME}"
    chmod +x "$INSTALL_DIR/${BINARY_NAME}"
    echo "Installed to ${INSTALL_DIR}/${BINARY_NAME}"
elif command -v sudo >/dev/null 2>&1; then
    sudo mv "$TMPDIR/${BINARY_NAME}" "$INSTALL_DIR/${BINARY_NAME}"
    sudo chmod +x "$INSTALL_DIR/${BINARY_NAME}"
    echo "Installed to ${INSTALL_DIR}/${BINARY_NAME}"
else
    FALLBACK_DIR="$HOME/.local/bin"
    mkdir -p "$FALLBACK_DIR"
    mv "$TMPDIR/${BINARY_NAME}" "$FALLBACK_DIR/${BINARY_NAME}"
    chmod +x "$FALLBACK_DIR/${BINARY_NAME}"
    echo "Installed to ${FALLBACK_DIR}/${BINARY_NAME}"
    echo "Add to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\""
fi

echo ""
echo "Run: ${BINARY_NAME} --help"
