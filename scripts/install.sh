#!/bin/sh
set -eu

# hive install script (Unix: Linux / macOS)
# Usage: curl -fsSL <raw-url> | sh

BIN="hive"
BASE_URL="https://PaRr0tBoY.github.io/product/Hive/install"
LATEST_JSON_URL="${BASE_URL}/latest.json"
INSTALL_DIR="${HIVE_INSTALL_DIR:-$HOME/.local/bin}"

main() {
    echo ""
    echo "      hive installer (fork)"
    echo "      ${BASE_URL}"
    echo ""

    # ---- detect platform ----
    OS="$(uname -s)"
    case "$OS" in
        Linux)  os="linux" ;;
        Darwin) os="macos" ;;
        MSYS*|MINGW*|CYGWIN*) err "Git Bash / MSYS detected. Use PowerShell instead: irm ${BASE_URL}/install.ps1 | iex" ;;
        *)                    err "unsupported OS: $OS (Linux and macOS only)" ;;
    esac

    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)  arch="aarch64" ;;
        *)              err "unsupported architecture: $ARCH" ;;
    esac

    target="${os}-${arch}"
    log "detected ${target}"

    need curl

    # ---- fetch latest manifest ----
    log "fetching latest release manifest..."
    manifest="$(curl -fsSL --retry 3 --connect-timeout 10 --max-time 20 "$LATEST_JSON_URL")" \
        || err "can't reach $LATEST_JSON_URL. Check your connection."

    # Parse version
    version="$(printf '%s\n' "$manifest" | grep -o '"version":"[^"]*"' | head -1 | sed 's/"version":"\([^"]*\)"/\1/')"
    if [ -z "$version" ]; then
        err "could not parse version from manifest"
    fi

    # Extract the asset block for this platform: "linux-x86_64": { "url": "...", "sha256": "..." }
    asset_block="$(printf '%s\n' "$manifest" | sed -n '/"'"${target}"'": {/,/}/p')"
    download_url="$(printf '%s\n' "$asset_block" | grep -o '"url":"[^"]*"' | head -1 | sed 's/"url":"\([^"]*\)"/\1/')"
    expected_sha256="$(printf '%s\n' "$asset_block" | grep -o '"sha256":"[^"]*"' | head -1 | sed 's/"sha256":"\([^"]*\)"/\1/')"

    if [ -z "$download_url" ]; then
        err "no asset '${target}' found in manifest. Is this platform built?"
    fi

    # ---- download ----
    log "downloading Hive ${version} (${target})..."
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT

    if ! curl -fsSL --retry 3 --connect-timeout 10 --max-time 300 \
        "$download_url" -o "${TMP}/${BIN}"; then
        err "download failed"
    fi

    # ---- sha256 verification ----
    if [ -n "$expected_sha256" ]; then
        log "verifying checksum..."
        if command -v sha256sum >/dev/null 2>&1; then
            actual="$(sha256sum "${TMP}/${BIN}" | awk '{print $1}')"
        else
            actual="$(shasum -a 256 "${TMP}/${BIN}" | awk '{print $1}')"
        fi
        if [ "$expected_sha256" != "$actual" ]; then
            err "SHA-256 mismatch. Expected $expected_sha256, got $actual"
        fi
        log "checksum verified"
    else
        warn "No checksum in manifest; skipping verification"
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
    log "done. run 'hive' to start."
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
