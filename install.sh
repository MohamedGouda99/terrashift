#!/usr/bin/env sh
# Terrashift — one-line installer for Linux + macOS.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/MohamedGouda99/terrashift/main/install.sh | sh
#
# Or, with a specific version:
#   curl -sSL https://raw.githubusercontent.com/MohamedGouda99/terrashift/main/install.sh | sh -s -- --version v0.1.0
#
# Or, into a custom prefix:
#   curl -sSL https://raw.githubusercontent.com/MohamedGouda99/terrashift/main/install.sh | sh -s -- --prefix /usr/local/bin
#
# Copyright (c) 2026 Mohamed Gouda. All rights reserved.
# Licensed under the Terrashift Source-Available License v1.0 — see
# https://github.com/MohamedGouda99/terrashift/blob/main/LICENSE.

set -eu

# ─────────────────────────────────────────────────────────────────────
# Defaults — overridable via flags or env vars
# ─────────────────────────────────────────────────────────────────────
REPO="${TERRASHIFT_REPO:-MohamedGouda99/terrashift}"
PREFIX="${TERRASHIFT_PREFIX:-$HOME/.local/bin}"
VERSION="${TERRASHIFT_VERSION:-latest}"
BINARY_NAME="terrashift"
TMPDIR="${TMPDIR:-/tmp}"

# ─────────────────────────────────────────────────────────────────────
# Argument parsing — keep it small; no getopts to stay POSIX-portable
# ─────────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            VERSION="$2"
            shift 2
            ;;
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        --help|-h)
            cat <<EOF
Terrashift installer

Options:
  --version <tag>   Install a specific version (e.g. v0.1.0). Default: latest.
  --prefix <dir>    Install destination. Default: \$HOME/.local/bin.
  --help            Show this help.

Environment overrides:
  TERRASHIFT_VERSION  same as --version
  TERRASHIFT_PREFIX   same as --prefix
  TERRASHIFT_REPO     GitHub repo to download from (default: MohamedGouda99/terrashift).
EOF
            exit 0
            ;;
        *)
            echo "[install] unknown argument: $1" >&2
            echo "[install] try --help" >&2
            exit 2
            ;;
    esac
done

# ─────────────────────────────────────────────────────────────────────
# Pretty output helpers
# ─────────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
    BOLD="$(printf '\033[1m')"
    GREEN="$(printf '\033[32m')"
    YELLOW="$(printf '\033[33m')"
    RED="$(printf '\033[31m')"
    RESET="$(printf '\033[0m')"
else
    BOLD=""
    GREEN=""
    YELLOW=""
    RED=""
    RESET=""
fi

say()  { printf '%s[install]%s %s\n' "$BOLD" "$RESET" "$1"; }
ok()   { printf '%s[install]%s %s%s%s\n' "$BOLD" "$RESET" "$GREEN" "$1" "$RESET"; }
warn() { printf '%s[install]%s %s%s%s\n' "$BOLD" "$RESET" "$YELLOW" "$1" "$RESET"; }
die()  { printf '%s[install]%s %sERROR:%s %s\n' "$BOLD" "$RESET" "$RED" "$RESET" "$1" >&2; exit 1; }

# ─────────────────────────────────────────────────────────────────────
# Tool detection — prefer curl, fall back to wget
# ─────────────────────────────────────────────────────────────────────
if command -v curl >/dev/null 2>&1; then
    DOWNLOADER="curl"
elif command -v wget >/dev/null 2>&1; then
    DOWNLOADER="wget"
else
    die "neither curl nor wget is installed. Install one and re-run."
fi

fetch_to_stdout() {
    # $1 = url
    case "$DOWNLOADER" in
        curl) curl -fsSL "$1" ;;
        wget) wget -qO- "$1" ;;
    esac
}

fetch_to_file() {
    # $1 = url, $2 = output path
    case "$DOWNLOADER" in
        curl) curl -fsSL -o "$2" "$1" ;;
        wget) wget -qO "$2" "$1" ;;
    esac
}

# Need tar for the tarball.
command -v tar >/dev/null 2>&1 \
    || die "tar is not installed. Install it and re-run."

# Need either sha256sum (Linux) or shasum (macOS) for verification.
if command -v sha256sum >/dev/null 2>&1; then
    SHA_CMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    SHA_CMD="shasum -a 256"
else
    warn "neither sha256sum nor shasum found — checksum verification will be skipped."
    SHA_CMD=""
fi

# ─────────────────────────────────────────────────────────────────────
# Detect platform → map to release artifact name
# ─────────────────────────────────────────────────────────────────────
OS="$(uname -s 2>/dev/null || echo Unknown)"
ARCH="$(uname -m 2>/dev/null || echo Unknown)"

case "$OS" in
    Linux*)   PLATFORM="linux"  ;;
    Darwin*)  PLATFORM="darwin" ;;
    *)        die "unsupported OS: $OS. Terrashift ships Linux and macOS binaries today.
       Windows users: run this installer inside WSL2, or build from source:
           git clone https://github.com/${REPO}.git && cd terrashift && cargo build --release" ;;
esac

case "$ARCH" in
    x86_64|amd64)   ARCH_TAG="x86_64"   ;;
    aarch64|arm64)  ARCH_TAG="aarch64"  ;;
    *)              die "unsupported architecture: $ARCH" ;;
esac

# Today's release matrix (see .github/workflows/release.yml):
#   linux-x86_64-musl
#   darwin-aarch64
# Map platform×arch → artifact name. If a combination isn't released yet,
# fail with a clear message instead of silently 404'ing.
ARTIFACT=""
case "${PLATFORM}-${ARCH_TAG}" in
    linux-x86_64)   ARTIFACT="terrashift-linux-x86_64" ;;
    darwin-aarch64) ARTIFACT="terrashift-darwin-aarch64" ;;
    *)
        die "no released artifact for ${PLATFORM}-${ARCH_TAG}.
       Today's matrix: linux-x86_64, darwin-aarch64.
       To request a build target, open an issue at
       https://github.com/${REPO}/issues."
        ;;
esac

say "platform: ${PLATFORM}-${ARCH_TAG} → artifact: ${ARTIFACT}"

# ─────────────────────────────────────────────────────────────────────
# Resolve version
# ─────────────────────────────────────────────────────────────────────
if [ "$VERSION" = "latest" ]; then
    say "resolving latest release from GitHub API..."
    # GitHub API redirects /releases/latest to the actual release. We
    # fetch the JSON and extract the tag_name with grep + sed (no jq
    # dependency).
    LATEST_JSON_URL="https://api.github.com/repos/${REPO}/releases/latest"
    # Buffer the JSON into a variable first. If we piped curl directly
    # into `grep -m 1`, grep would close stdin after the first match,
    # SIGPIPE'ing curl, which then prints "curl: (23) Failure writing
    # output to destination" — confusing for users even though TAG
    # extraction succeeds.
    LATEST_JSON="$(fetch_to_stdout "$LATEST_JSON_URL" || true)"
    TAG="$(printf '%s' "$LATEST_JSON" \
        | grep -m 1 '"tag_name":' \
        | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
    if [ -z "$TAG" ]; then
        die "no published releases found at https://github.com/${REPO}/releases.
       This usually means either no version has been tagged yet, or
       the repository is private (release assets require auth).

       To install once a release is published, re-run this script.
       To build from source now:
           git clone https://github.com/${REPO}.git
           cd terrashift && cargo build --release --bin terrashift"
    fi
    VERSION="$TAG"
fi
say "installing version: ${VERSION}"

# ─────────────────────────────────────────────────────────────────────
# Download tarball + checksum
# ─────────────────────────────────────────────────────────────────────
TARBALL="${ARTIFACT}.tar.gz"
CHECKSUM="${ARTIFACT}.tar.gz.sha256"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
TARBALL_URL="${BASE_URL}/${TARBALL}"
CHECKSUM_URL="${BASE_URL}/${CHECKSUM}"

WORKDIR="$(mktemp -d "${TMPDIR}/terrashift-install.XXXXXX")"
trap 'rm -rf "$WORKDIR"' EXIT INT TERM

say "downloading $TARBALL_URL"
fetch_to_file "$TARBALL_URL" "${WORKDIR}/${TARBALL}" \
    || die "download failed. Check version tag and network."

if [ -n "$SHA_CMD" ]; then
    say "downloading checksum"
    if fetch_to_file "$CHECKSUM_URL" "${WORKDIR}/${CHECKSUM}" 2>/dev/null; then
        say "verifying SHA256"
        cd "$WORKDIR"
        # The checksum file is in the format: <hash>  <filename>
        # `sha256sum -c` checks both. macOS shasum supports the same.
        if ! $SHA_CMD -c "$CHECKSUM" >/dev/null 2>&1; then
            die "SHA256 checksum mismatch. Tarball may be corrupted or tampered with."
        fi
        cd - >/dev/null
        ok "checksum verified"
    else
        warn "checksum file not found — proceeding without verification."
    fi
fi

# ─────────────────────────────────────────────────────────────────────
# Extract + install
# ─────────────────────────────────────────────────────────────────────
say "extracting"
tar -xzf "${WORKDIR}/${TARBALL}" -C "$WORKDIR" \
    || die "tar extract failed."

if [ ! -f "${WORKDIR}/${BINARY_NAME}" ]; then
    die "expected binary ${BINARY_NAME} not found inside the tarball."
fi

mkdir -p "$PREFIX" \
    || die "could not create prefix directory: $PREFIX"

INSTALL_PATH="${PREFIX}/${BINARY_NAME}"
say "installing to $INSTALL_PATH"
mv "${WORKDIR}/${BINARY_NAME}" "$INSTALL_PATH" \
    || die "install failed (need write access to $PREFIX?)"
chmod +x "$INSTALL_PATH"

# ─────────────────────────────────────────────────────────────────────
# Verify + PATH guidance
# ─────────────────────────────────────────────────────────────────────
INSTALLED_VERSION="$("$INSTALL_PATH" version 2>/dev/null | head -n 1 || true)"
ok "installed: $INSTALLED_VERSION"

case ":$PATH:" in
    *":$PREFIX:"*)
        ok "$PREFIX is on your PATH — run \`terrashift\` to launch the TUI."
        ;;
    *)
        # Auto-configure PATH so a fresh terminal Just Works. Pattern
        # matches rustup / nvm / ollama: append once to the user's
        # shell rc file, idempotent on re-runs (we look for the exact
        # export line). Skip auto-add for fish (different syntax).
        SHELL_NAME="$(basename "${SHELL:-/bin/sh}")"
        case "$SHELL_NAME" in
            fish)
                warn "$PREFIX is NOT on your PATH."
                printf '\n        Fish-shell users — add it manually:\n\n'
                printf '            fish_add_path "%s"\n\n' "$PREFIX"
                ;;
            *)
                case "$SHELL_NAME" in
                    zsh)  RC_FILE="$HOME/.zshrc"   ;;
                    bash) RC_FILE="$HOME/.bashrc"  ;;
                    *)    RC_FILE="$HOME/.profile" ;;
                esac
                EXPORT_LINE="export PATH=\"$PREFIX:\$PATH\""
                if [ -f "$RC_FILE" ] && grep -Fxq "$EXPORT_LINE" "$RC_FILE"; then
                    ok "$PREFIX already configured in $RC_FILE — open a new terminal."
                else
                    {
                        printf '\n# Added by terrashift installer\n'
                        printf '%s\n' "$EXPORT_LINE"
                    } >> "$RC_FILE"
                    ok "added $PREFIX to PATH in $RC_FILE."
                    say "Open a new terminal, or run: source $RC_FILE"
                fi
                ;;
        esac
        ;;
esac

ok "done. Try: terrashift --help"
