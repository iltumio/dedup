#!/bin/sh
set -eu

REPO="iltumio/dedup"
BINARY="dedup"
INSTALL_DIR="/usr/local/bin"

main() {
    need_cmd curl
    need_cmd uname

    os="$(detect_os)"
    arch="$(detect_arch)"
    target="$(map_target "$os" "$arch")"

    if [ -z "$target" ]; then
        err "unsupported platform: ${os}/${arch}"
    fi

    ext=""
    if [ "$os" = "windows" ]; then
        ext=".exe"
    fi

    asset="dedup-${target}${ext}"
    url="$(get_latest_release_url "$asset")"

    if [ -z "$url" ]; then
        err "could not find release asset: ${asset}"
    fi

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    info "Downloading ${asset}..."
    download "$url" "${tmpdir}/${BINARY}${ext}"
    chmod +x "${tmpdir}/${BINARY}${ext}"

    install_dir="$INSTALL_DIR"
    if [ "$os" = "windows" ]; then
        install_dir="${USERPROFILE:-$HOME}/bin"
    fi

    if [ -w "$install_dir" ] 2>/dev/null; then
        mv "${tmpdir}/${BINARY}${ext}" "${install_dir}/${BINARY}${ext}"
    elif command -v sudo >/dev/null 2>&1; then
        info "Installing to ${install_dir} (requires sudo)..."
        sudo mv "${tmpdir}/${BINARY}${ext}" "${install_dir}/${BINARY}${ext}"
    else
        err "cannot write to ${install_dir} — run with sudo or set INSTALL_DIR"
    fi

    info "Installed ${BINARY} to ${install_dir}/${BINARY}${ext}"

    if command -v "$BINARY" >/dev/null 2>&1; then
        info "Version: $("$BINARY" --version 2>/dev/null || echo 'unknown')"
    else
        warn "${install_dir} is not in your PATH — add it to use '${BINARY}' directly"
    fi
}

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT) echo "windows" ;;
        *) err "unsupported OS: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        arm64|aarch64) echo "aarch64" ;;
        *) err "unsupported architecture: $(uname -m)" ;;
    esac
}

map_target() {
    os="$1"
    arch="$2"

    case "${os}-${arch}" in
        linux-x86_64)   echo "x86_64-unknown-linux-gnu" ;;
        macos-aarch64)  echo "aarch64-apple-darwin" ;;
        macos-x86_64)   echo "x86_64-apple-darwin" ;;
        windows-x86_64) echo "x86_64-pc-windows-msvc" ;;
        *) echo "" ;;
    esac
}

get_latest_release_url() {
    asset_name="$1"
    release_url="https://api.github.com/repos/${REPO}/releases/latest"
    url="$(curl -fsSL "$release_url" \
        | grep -o "\"browser_download_url\":[[:space:]]*\"[^\"]*${asset_name}\"" \
        | grep -o 'https://[^"]*')" || true
    echo "$url"
}

download() {
    url="$1"
    dest="$2"
    curl -fsSL -o "$dest" "$url"
}

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "required command not found: $1"
    fi
}

info() {
    printf '\033[1;32m=>\033[0m %s\n' "$1"
}

warn() {
    printf '\033[1;33mwarning:\033[0m %s\n' "$1"
}

err() {
    printf '\033[1;31merror:\033[0m %s\n' "$1" >&2
    exit 1
}

main "$@"
