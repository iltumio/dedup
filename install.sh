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

    choice="$(ask_component)"

    case "$choice" in
        cli) install_cli "$os" "$arch" ;;
        app) install_app "$os" "$arch" ;;
        *)   err "invalid choice" ;;
    esac
}

ask_component() {
    printf '\n' >&2
    info "What would you like to install?"
    printf '  1) CLI  — command-line tool\n' >&2
    printf '  2) App  — desktop application (Tauri)\n' >&2
    printf '\n' >&2
    printf 'Enter choice [1/2]: ' >&2
    read -r answer </dev/tty
    case "$answer" in
        1|cli)  echo "cli" ;;
        2|app)  echo "app" ;;
        *)      err "invalid choice: ${answer}. Please enter 1 or 2." ;;
    esac
}

# ---------------------------------------------------------------------------
# CLI installation
# ---------------------------------------------------------------------------

install_cli() {
    os="$1"
    arch="$2"
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

# ---------------------------------------------------------------------------
# App installation
# ---------------------------------------------------------------------------

install_app() {
    os="$1"
    arch="$2"
    target="$(map_target "$os" "$arch")"

    if [ -z "$target" ]; then
        err "unsupported platform: ${os}/${arch}"
    fi

    url="$(get_latest_app_url "$os" "$arch")"

    if [ -z "$url" ]; then
        err "could not find desktop app release for ${os}/${arch}. Check https://github.com/${REPO}/releases"
    fi

    filename="$(basename "$url")"
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    info "Downloading ${filename}..."
    download "$url" "${tmpdir}/${filename}"

    case "$os" in
        linux)
            install_app_linux "${tmpdir}/${filename}"
            ;;
        macos)
            install_app_macos "${tmpdir}/${filename}"
            ;;
        windows)
            install_app_windows "${tmpdir}/${filename}"
            ;;
    esac
}

install_app_linux() {
    appimage="$1"
    chmod +x "$appimage"

    dest_dir="${HOME}/.local/bin"
    mkdir -p "$dest_dir"
    dest="${dest_dir}/dedup-app.AppImage"
    mv "$appimage" "$dest"

    info "Installed AppImage to ${dest}"
    info "Run with: ${dest}"

    if ! echo "$PATH" | grep -q "${dest_dir}"; then
        warn "${dest_dir} is not in your PATH — add it or run the AppImage directly"
    fi
}

install_app_macos() {
    dmg="$1"
    need_cmd hdiutil

    info "Mounting disk image..."
    mount_dir="$(mktemp -d)"
    hdiutil attach -quiet -mountpoint "$mount_dir" "$dmg"

    app="$(find "$mount_dir" -maxdepth 1 -name '*.app' | head -1)"
    if [ -z "$app" ]; then
        hdiutil detach -quiet "$mount_dir"
        err "no .app bundle found in disk image"
    fi

    dest="/Applications/$(basename "$app")"
    if [ -d "$dest" ]; then
        info "Removing existing installation..."
        rm -rf "$dest"
    fi

    info "Copying to /Applications..."
    cp -R "$app" /Applications/

    hdiutil detach -quiet "$mount_dir"
    info "Installed to ${dest}"
}

install_app_windows() {
    installer="$1"
    info "Launching installer..."
    info "Please follow the installation wizard."
    cmd.exe /C "$(cygpath -w "$installer")" || "$installer"
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

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

get_latest_app_url() {
    os="$1"
    arch="$2"
    release_url="https://api.github.com/repos/${REPO}/releases/latest"

    urls="$(curl -fsSL "$release_url" \
        | grep -o '"browser_download_url":[[:space:]]*"[^"]*"' \
        | grep -o 'https://[^"]*')" || true

    case "${os}" in
        linux)
            echo "$urls" | grep -i '\.AppImage$' | head -1
            ;;
        macos)
            echo "$urls" | grep -i '\.dmg$' | head -1
            ;;
        windows)
            echo "$urls" | grep -i 'nsis.*\.exe$\|setup.*\.exe$' | head -1 \
                || echo "$urls" | grep -i '\.msi$' | head -1
            ;;
    esac
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
    printf '\033[1;32m=>\033[0m %s\n' "$1" >&2
}

warn() {
    printf '\033[1;33mwarning:\033[0m %s\n' "$1" >&2
}

err() {
    printf '\033[1;31merror:\033[0m %s\n' "$1" >&2
    exit 1
}

main "$@"
