#!/bin/sh
set -e

REPO="youming-ai/agent-usage-monitor"
BINARY_NAME="aum"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Release signing public key — must match MINISIGN_PUBLIC_KEY in
# src/updater/mod.rs and the MINISIGN_SECRET_KEY secret used in
# .github/workflows/release.yml.
MINISIGN_PUBLIC_KEY="RWSJYQ0u3cwMksoh3aAd0tTZF1GbxroMEF6FqPY+KjtCU2OWp7bmcaa8"

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

ASSET_NAME="aum-${OS_NAME}-${ARCH_NAME}.tar.gz"
# Escape regex metacharacters (just the dots) so the match below anchors on
# the literal asset name instead of treating '.' as "any character".
ASSET_NAME_RE=$(printf '%s' "$ASSET_NAME" | sed 's/\./\\./g')

# Get latest release info once, and extract both asset URLs from the same
# JSON — avoids two round trips and keeps the two lookups consistent.
if command -v curl >/dev/null 2>&1; then
    RELEASE_JSON=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest")
elif command -v wget >/dev/null 2>&1; then
    RELEASE_JSON=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest")
else
    echo "Error: curl or wget is required"
    exit 1
fi

# Anchored so the match ends exactly at the asset name (a closing quote right
# after it) — a plain substring grep would let a maliciously-named asset
# (e.g. "${ASSET_NAME}.evil") win via `head -1` before the real one.
DOWNLOAD_URL=$(printf '%s\n' "$RELEASE_JSON" \
    | grep -o "\"browser_download_url\": *\"[^\"]*/${ASSET_NAME_RE}\"" \
    | head -1 \
    | cut -d '"' -f 4)
SIG_URL=$(printf '%s\n' "$RELEASE_JSON" \
    | grep -o "\"browser_download_url\": *\"[^\"]*/${ASSET_NAME_RE}\\.minisig\"" \
    | head -1 \
    | cut -d '"' -f 4)

if [ -z "$DOWNLOAD_URL" ]; then
    echo "Error: Could not find release asset for ${OS_NAME}/${ARCH_NAME}"
    echo "Check https://github.com/${REPO}/releases for available downloads"
    exit 1
fi

echo "Downloading ${BINARY_NAME} from ${DOWNLOAD_URL}..."

# Renamed from the previous "TMPDIR" so this script's working directory
# doesn't shadow the standard TMPDIR environment variable (which mktemp and
# other tools consult to decide *where* to create it).
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o "$WORK_DIR/${ASSET_NAME}"
else
    wget -qO "$WORK_DIR/${ASSET_NAME}" "$DOWNLOAD_URL"
fi

# Verify the download against its detached minisign signature before doing
# anything with it. The CLI installer can't embed a key as tamper-evidently
# as the compiled binary can (anyone can edit this script), so this is
# best-effort: verify when `minisign` is available, warn loudly and continue
# when it isn't, but always hard-fail on an actual signature mismatch.
if [ -z "$SIG_URL" ]; then
    echo "Warning: release is missing a .minisig signature asset; skipping verification." >&2
elif command -v minisign >/dev/null 2>&1; then
    echo "Verifying signature..."
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$SIG_URL" -o "$WORK_DIR/${ASSET_NAME}.minisig"
    else
        wget -qO "$WORK_DIR/${ASSET_NAME}.minisig" "$SIG_URL"
    fi
    if ! minisign -Vm "$WORK_DIR/${ASSET_NAME}" -P "$MINISIGN_PUBLIC_KEY"; then
        echo "Error: signature verification FAILED — refusing to install a tampered or corrupted download." >&2
        exit 1
    fi
    echo "Signature OK."
else
    echo "Warning: 'minisign' is not installed, so the download signature was NOT verified." >&2
    echo "  Install it to verify releases: https://jedisct1.github.io/minisign/" >&2
    echo "  (e.g. 'brew install minisign' or 'apt install minisign')" >&2
fi

echo "Extracting..."
tar xzf "$WORK_DIR/${ASSET_NAME}" -C "$WORK_DIR"

# Try to install to INSTALL_DIR, fall back to ~/.local/bin
if [ -w "$INSTALL_DIR" ] 2>/dev/null; then
    mv "$WORK_DIR/${BINARY_NAME}" "$INSTALL_DIR/${BINARY_NAME}"
    chmod +x "$INSTALL_DIR/${BINARY_NAME}"
    echo "Installed to ${INSTALL_DIR}/${BINARY_NAME}"
elif command -v sudo >/dev/null 2>&1; then
    # root's primary group is "wheel" on macOS/BSD and "root" on Linux; get it
    # from the system instead of hardcoding either one.
    case "$OS" in
        Darwin) ROOT_GROUP="wheel" ;;
        *)      ROOT_GROUP="root" ;;
    esac
    # ponytail: `install -o root -g $ROOT_GROUP` sets ownership as part of the
    # same privileged operation that places the file, so the binary is never
    # momentarily (or permanently, if a step were interrupted) owned by the
    # unprivileged invoking user — `mv` + `chmod` left it user-owned, a
    # persistence/escalation foothold in a root-owned PATH directory.
    sudo install -m 755 -o root -g "$ROOT_GROUP" "$WORK_DIR/${BINARY_NAME}" "$INSTALL_DIR/${BINARY_NAME}"
    echo "Installed to ${INSTALL_DIR}/${BINARY_NAME}"
else
    FALLBACK_DIR="$HOME/.local/bin"
    mkdir -p "$FALLBACK_DIR"
    mv "$WORK_DIR/${BINARY_NAME}" "$FALLBACK_DIR/${BINARY_NAME}"
    chmod +x "$FALLBACK_DIR/${BINARY_NAME}"
    echo "Installed to ${FALLBACK_DIR}/${BINARY_NAME}"
    echo "Add to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\""
fi

echo ""
echo "Run: ${BINARY_NAME} --help"
