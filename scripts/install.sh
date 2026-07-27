#!/bin/sh
set -eu

# herdr install script (Unix: Linux / macOS)
# Downloads the latest release from PaRr0tBoY/herdr
# Usage: curl -fsSL <raw-url> | sh

BIN="herdr"
REPO="PaRr0tBoY/herdr"
INSTALL_DIR="${HERDR_INSTALL_DIR:-$HOME/.local/bin}"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"

main() {
    echo ""
    echo "      herdr installer (fork)"
    echo "      github.com/${REPO}"
    echo ""

    # ---- detect platform ----
    OS="$(uname -s)"
    case "$OS" in
        Linux)  os="linux" ;;
        Darwin) os="macos" ;;
        *)      err "unsupported OS: $OS (Linux and macOS only)" ;;
    esac

    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)  arch="aarch64" ;;
        *)              err "unsupported architecture: $ARCH" ;;
    esac

    target="${os}-${arch}"
    asset_name="${BIN}-${target}"
    log "detected ${target}"

    need curl

    # ---- fetch release info from GitHub API ----
    log "fetching latest release..."
    release_json="$(curl -fsSL --retry 3 --connect-timeout 10 --max-time 20 "$API_URL")" \
        || err "can't reach GitHub API. Check your connection."

    # Parse tag name (version)
    version="$(printf '%s\n' "$release_json" | tr -d '\n' | \
        grep -o '"tag_name":"[^"]*"' | head -1 | \
        sed 's/"tag_name":"\([^"]*\)"/\1/')"

    # Find the download URL for this platform's asset
    download_url="$(printf '%s\n' "$release_json" | tr -d '\n' | \
        grep -o '"browser_download_url":"[^"]*'"${asset_name}"'[^"]*"' | head -1 | \
        sed 's/"browser_download_url":"\([^"]*\)"/\1/')"

    if [ -z "$download_url" ]; then
        err "no asset '${asset_name}' found in release ${version}. Is this platform built?"
    fi

    # ---- download ----
    log "downloading ${version} (${asset_name})..."
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT

    if ! curl -fsSL --retry 3 --connect-timeout 10 --max-time 300 \
        "$download_url" -o "${TMP}/${BIN}"; then
        err "download failed"
    fi

    # ---- sha256 verification (if checksum file exists) ----
    sha256_url="${download_url}.sha256"
    if curl -fsSL --connect-timeout 5 --max-time 10 \
        "$sha256_url" -o "${TMP}/${BIN}.sha256" 2>/dev/null; then
        log "verifying checksum..."
        expected="$(awk '{print $1}' "${TMP}/${BIN}.sha256")"
        if command -v sha256sum >/dev/null 2>&1; then
            actual="$(sha256sum "${TMP}/${BIN}" | awk '{print $1}')"
        else
            actual="$(shasum -a 256 "${TMP}/${BIN}" | awk '{print $1}')"
        fi
        if [ "$expected" != "$actual" ]; then
            err "checksum mismatch"
        fi
        log "checksum ok"
    else
        warn "no checksum file; skipping verification"
    fi

    # ---- install ----
    mkdir -p "$INSTALL_DIR"
    mv "${TMP}/${BIN}" "${INSTALL_DIR}/${BIN}"
    chmod +x "${INSTALL_DIR}/${BIN}"

    log "installed to ${INSTALL_DIR}/${BIN}"

    # ---- PATH check ----
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo ""
            warn "${INSTALL_DIR} is not in your PATH"
            echo "  add this to your shell config:"
            echo ""
            echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
            echo ""
            ;;
    esac

    echo ""
    log "done. run 'herdr' to start."
    echo ""
}

log()  { printf '  \033[32m>\033[0m %s\n' "$1"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$1"; }
err()  { printf '  \033[31mx\033[0m %s\n' "$1" >&2; exit 1; }

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "requires '$1' — install it first"
    fi
}

main "$@"
