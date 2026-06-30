#!/usr/bin/env bash
#
# fmd installer — franken_markdown CLI
# Pure-Rust, dependency-lean Markdown -> beautiful self-contained HTML & tiny PDF.
#
# One-liner install:
#   curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.sh | bash
#
# With cache buster (bypass CDN caching of the script itself):
#   curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.sh?$(date +%s)" | bash
#
# Options:
#   --version vX.Y.Z   Install a specific version (default: latest release)
#   --dest DIR         Install to DIR (default: ~/.local/bin)
#   --system           Install to /usr/local/bin (requires sudo)
#   --easy-mode        Auto-update PATH in your shell rc files
#   --verify           Run a post-install self-test (fmd --version + render smoke test)
#   --from-source      Build from source with cargo instead of downloading a binary
#   --quiet            Suppress non-error output
#   --no-gum           Disable gum formatting even if gum is available
#   --force            Reinstall even if the requested version is already present
#   -h, --help         Show help and exit
#
# Notes:
#   * Tagged releases publish prebuilt `fmd` archives. The installer prefers
#     those archives and only builds from source when --from-source is requested
#     or no matching release asset exists for the current platform.
#   * Build-from-source needs the Rust toolchain (cargo). If cargo is missing the
#     installer prints clear rustup guidance and exits.
#   * Proxy support: set HTTPS_PROXY / HTTP_PROXY and every download honors it.
#
set -euo pipefail
umask 022
shopt -s lastpipe 2>/dev/null || true

# ─────────────────────────────────────────────────────────────────────────────
# Configuration / defaults
# ─────────────────────────────────────────────────────────────────────────────
OWNER="${OWNER:-Dicklesworthstone}"
REPO="${REPO:-franken_markdown}"
BINARY_NAME="${BINARY_NAME:-fmd}"

VERSION="${VERSION:-}"
DEST_DEFAULT="$HOME/.local/bin"
DEST="${DEST:-$DEST_DEFAULT}"
SYSTEM=0
EASY=0
QUIET=0
VERIFY=0
FROM_SOURCE=0
FORCE=0
NO_GUM=0

# Optional escape hatches (env or flag-overridable, intentionally undocumented in --help)
ARTIFACT_URL="${ARTIFACT_URL:-}"
CHECKSUM="${CHECKSUM:-}"
CHECKSUM_URL="${CHECKSUM_URL:-}"

# Sigstore / cosign (best-effort verification when cosign is present)
SIGSTORE_BUNDLE_URL="${SIGSTORE_BUNDLE_URL:-}"
COSIGN_IDENTITY_RE="${COSIGN_IDENTITY_RE:-https://github.com/${OWNER}/${REPO}/.*}"
COSIGN_OIDC_ISSUER="${COSIGN_OIDC_ISSUER:-https://token.actions.githubusercontent.com}"

LOCK_FILE="${TMPDIR:-/tmp}/${BINARY_NAME}-install.lock"
BUILT_FROM_SOURCE=0

# ─────────────────────────────────────────────────────────────────────────────
# Output stack: gum (https://github.com/charmbracelet/gum) with ANSI fallback
# ─────────────────────────────────────────────────────────────────────────────
HAS_GUM=0
if command -v gum &> /dev/null && [ -t 1 ]; then
  HAS_GUM=1
fi
USE_COLOR=1
if [ -n "${NO_COLOR:-}" ] || [ "${TERM:-}" = "dumb" ]; then
  USE_COLOR=0
  NO_GUM=1
fi

log() { [ "$QUIET" -eq 1 ] && return 0; echo -e "$@"; }

info() {
  [ "$QUIET" -eq 1 ] && return 0
  if [ "$HAS_GUM" -eq 1 ] && [ "$NO_GUM" -eq 0 ]; then
    gum style --foreground 39 "→ $*"
  elif [ "$USE_COLOR" -eq 1 ]; then
    echo -e "\033[0;34m→\033[0m $*"
  else
    echo "-> $*"
  fi
}

ok() {
  [ "$QUIET" -eq 1 ] && return 0
  if [ "$HAS_GUM" -eq 1 ] && [ "$NO_GUM" -eq 0 ]; then
    gum style --foreground 42 "✓ $*"
  elif [ "$USE_COLOR" -eq 1 ]; then
    echo -e "\033[0;32m✓\033[0m $*"
  else
    echo "[OK] $*"
  fi
}

warn() {
  if [ "$HAS_GUM" -eq 1 ] && [ "$NO_GUM" -eq 0 ]; then
    gum style --foreground 214 "⚠ $*"
  elif [ "$USE_COLOR" -eq 1 ]; then
    echo -e "\033[1;33m⚠\033[0m $*"
  else
    echo "[!] $*"
  fi
}

err() {
  # Errors are never silenced, even with --quiet.
  if [ "$HAS_GUM" -eq 1 ] && [ "$NO_GUM" -eq 0 ]; then
    gum style --foreground 196 "✗ $*"
  elif [ "$USE_COLOR" -eq 1 ]; then
    echo -e "\033[0;31m✗\033[0m $*" >&2
  else
    echo "[X] $*" >&2
  fi
}

run_with_spinner() {
  local title="$1"; shift
  if [ "$HAS_GUM" -eq 1 ] && [ "$NO_GUM" -eq 0 ] && [ "$QUIET" -eq 0 ]; then
    gum spin --spinner dot --title "$title" -- "$@"
  else
    info "$title"
    "$@"
  fi
}

# draw_box COLOR LINE [LINE ...] — ANSI-aware bordered box (gum-free fallback).
draw_box() {
  local color="$1"; shift
  local lines=("$@")
  local max_width=0 esc
  esc=$(printf '\033')
  local strip_ansi_sed="s/${esc}\\[[0-9;]*m//g"

  local line stripped len
  for line in "${lines[@]}"; do
    stripped=$(printf '%b' "$line" | LC_ALL=C sed "$strip_ansi_sed")
    len=${#stripped}
    [ "$len" -gt "$max_width" ] && max_width=$len
  done

  local inner_width=$((max_width + 4))
  local border="" i
  for ((i=0; i<inner_width; i++)); do border+="═"; done

  if [ "$USE_COLOR" -eq 1 ]; then
    printf "\033[%sm╔%s╗\033[0m\n" "$color" "$border"
  else
    printf "╔%s╗\n" "$border"
  fi
  for line in "${lines[@]}"; do
    stripped=$(printf '%b' "$line" | LC_ALL=C sed "$strip_ansi_sed")
    len=${#stripped}
    local padding=$((max_width - len)) pad_str=""
    for ((i=0; i<padding; i++)); do pad_str+=" "; done
    if [ "$USE_COLOR" -eq 1 ]; then
      printf "\033[%sm║\033[0m  %b%s  \033[%sm║\033[0m\n" "$color" "$line" "$pad_str" "$color"
    else
      printf "║  %s%s  ║\n" "$stripped" "$pad_str"
    fi
  done
  if [ "$USE_COLOR" -eq 1 ]; then
    printf "\033[%sm╚%s╝\033[0m\n" "$color" "$border"
  else
    printf "╚%s╝\n" "$border"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Proxy support — applied to EVERY network request via the xcurl wrapper.
# ─────────────────────────────────────────────────────────────────────────────
PROXY_ARGS=()
setup_proxy() {
  PROXY_ARGS=()
  if [ -n "${HTTPS_PROXY:-${https_proxy:-}}" ]; then
    PROXY_ARGS=(--proxy "${HTTPS_PROXY:-$https_proxy}")
  elif [ -n "${HTTP_PROXY:-${http_proxy:-}}" ]; then
    PROXY_ARGS=(--proxy "${HTTP_PROXY:-$http_proxy}")
  fi
}

# Proxy-aware curl wrapper. The `[@]+` guard keeps an empty PROXY_ARGS safe under
# `set -u` on bash 3.2 (the macOS system bash).
xcurl() {
  curl ${PROXY_ARGS[@]+"${PROXY_ARGS[@]}"} "$@"
}

# ─────────────────────────────────────────────────────────────────────────────
# Usage
# ─────────────────────────────────────────────────────────────────────────────
usage() {
  cat <<EOFU
fmd installer — franken_markdown CLI (Markdown -> HTML & PDF)

Usage:
  install.sh [options]

Options:
  --version vX.Y.Z   Install a specific version (default: latest release)
  --dest DIR         Install to DIR (default: ~/.local/bin)
  --system           Install to /usr/local/bin (requires sudo)
  --easy-mode        Auto-update PATH in your shell rc files (~/.zshrc, ~/.bashrc)
  --verify           Run a post-install self-test (fmd --version + render smoke test)
  --from-source      Build from source with cargo instead of downloading a binary
  --quiet, -q        Suppress non-error output
  --no-gum           Disable gum formatting even if gum is available
  --force            Reinstall even if the requested version is already present
  -h, --help         Show this help and exit

Environment:
  HTTPS_PROXY / HTTP_PROXY   Routed through every download
  ARTIFACT_URL               Override the binary artifact URL
  CHECKSUM / CHECKSUM_URL     Provide/override the SHA256 checksum source

Examples:
  install.sh                       # latest, ~/.local/bin
  install.sh --system              # /usr/local/bin (sudo)
  install.sh --from-source --verify
  install.sh --version v0.1.0 --easy-mode
EOFU
}

# ─────────────────────────────────────────────────────────────────────────────
# Argument parsing
# ─────────────────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:-}"; shift 2;;
    --dest) DEST="${2:-}"; shift 2;;
    --system) SYSTEM=1; DEST="/usr/local/bin"; shift;;
    --easy-mode) EASY=1; shift;;
    --verify) VERIFY=1; shift;;
    --from-source) FROM_SOURCE=1; shift;;
    --quiet|-q) QUIET=1; shift;;
    --no-gum) NO_GUM=1; shift;;
    --no-color|--no-colour) USE_COLOR=0; NO_GUM=1; shift;;
    --force) FORCE=1; shift;;
    --artifact-url) ARTIFACT_URL="${2:-}"; shift 2;;
    --checksum) CHECKSUM="${2:-}"; shift 2;;
    --checksum-url) CHECKSUM_URL="${2:-}"; shift 2;;
    -h|--help) usage; exit 0;;
    *) warn "Ignoring unknown option: $1"; shift;;
  esac
done

# ─────────────────────────────────────────────────────────────────────────────
# Branded header banner
# ─────────────────────────────────────────────────────────────────────────────
if [ "$QUIET" -eq 0 ]; then
  if [ "$HAS_GUM" -eq 1 ] && [ "$NO_GUM" -eq 0 ]; then
    gum style \
      --border rounded --border-foreground 39 \
      --padding "0 2" --margin "1 0" \
      "$(gum style --foreground 42 --bold '🧟 fmd installer')" \
      "$(gum style --foreground 245 'franken_markdown — Markdown to beautiful HTML & tiny PDF')"
  else
    echo ""
    if [ "$USE_COLOR" -eq 1 ]; then
      draw_box "1;36" \
        "$(printf '\033[1;32m🧟 fmd installer\033[0m')" \
        "$(printf '\033[0;90mfranken_markdown — Markdown to beautiful HTML & tiny PDF\033[0m')"
    else
      draw_box "1;36" \
        "🧟 fmd installer" \
        "franken_markdown — Markdown to beautiful HTML & tiny PDF"
    fi
    echo ""
  fi
fi

setup_proxy

# ─────────────────────────────────────────────────────────────────────────────
# Platform detection (OS x ARCH -> release Rust triple, WSL warning)
# ─────────────────────────────────────────────────────────────────────────────
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *) warn "Unknown architecture '$ARCH'; will fall back to building from source" ;;
esac

TARGET=""
EXT="tar.gz"
case "${OS}-${ARCH}" in
  linux-x86_64)   TARGET="x86_64-unknown-linux-gnu" ;;
  linux-aarch64)  TARGET="aarch64-unknown-linux-gnu" ;;
  darwin-x86_64)  TARGET="x86_64-apple-darwin" ;;
  darwin-aarch64) TARGET="aarch64-apple-darwin" ;;
  *) warn "No prebuilt target for ${OS}/${ARCH}; will build from source"; FROM_SOURCE=1 ;;
esac

# WSL detection — warn but continue with the Linux path.
if [ "$OS" = "linux" ] && grep -qi microsoft /proc/version 2>/dev/null; then
  warn "WSL detected — installing the Linux build. Add $DEST to your PATH inside WSL."
fi

VERSION_BARE="${VERSION#v}"

# ─────────────────────────────────────────────────────────────────────────────
# Version resolution: --version flag -> GitHub API latest -> redirect parse.
# Never hard-exits: with no releases yet, an empty VERSION simply routes to the
# from-source fallback.
# ─────────────────────────────────────────────────────────────────────────────
resolve_version() {
  if [ -n "$VERSION" ]; then
    info "Using requested version: $VERSION"
    return 0
  fi
  if [ "$FROM_SOURCE" -eq 1 ]; then
    return 0
  fi

  info "Resolving latest version…"
  local tag=""
  tag=$(xcurl -fsSL --connect-timeout 15 --max-time 30 \
        -H "Accept: application/vnd.github.v3+json" \
        "https://api.github.com/repos/${OWNER}/${REPO}/releases/latest" 2>/dev/null \
        | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' | head -n1 || true)

  if [ -z "$tag" ]; then
    # Fallback: follow the /releases/latest redirect and parse the tag.
    tag=$(xcurl -fsSL --connect-timeout 15 --max-time 30 -o /dev/null \
          -w '%{url_effective}' \
          "https://github.com/${OWNER}/${REPO}/releases/latest" 2>/dev/null \
          | sed -E 's|.*/tag/||' || true)
    case "$tag" in
      v[0-9]*) : ;;   # looks like a tag
      *) tag="" ;;    # redirected back to /releases (no release published)
    esac
  fi

  if [ -n "$tag" ]; then
    VERSION="$tag"
    VERSION_BARE="${VERSION#v}"
    info "Latest version: $VERSION"
  else
    warn "No published release found; will build from source"
    FROM_SOURCE=1
  fi
}

resolve_version

# ─────────────────────────────────────────────────────────────────────────────
# Atomic locking (mkdir-based; portable to macOS) with stale-PID detection
# ─────────────────────────────────────────────────────────────────────────────
LOCK_DIR="${LOCK_FILE}.d"
LOCKED=0
acquire_lock() {
  if mkdir "$LOCK_DIR" 2>/dev/null; then
    LOCKED=1; echo $$ > "$LOCK_DIR/pid"; return 0
  fi
  if [ -f "$LOCK_DIR/pid" ]; then
    local old_pid; old_pid=$(cat "$LOCK_DIR/pid" 2>/dev/null || echo "")
    if [ -n "$old_pid" ] && ! kill -0 "$old_pid" 2>/dev/null; then
      warn "Removing stale install lock (pid $old_pid is gone)"
      rm -rf "$LOCK_DIR"
      if mkdir "$LOCK_DIR" 2>/dev/null; then
        LOCKED=1; echo $$ > "$LOCK_DIR/pid"; return 0
      fi
    fi
  fi
  err "Another ${BINARY_NAME} installer is already running (lock: $LOCK_DIR)"
  exit 1
}
acquire_lock

# ─────────────────────────────────────────────────────────────────────────────
# Temp workspace + cleanup
# ─────────────────────────────────────────────────────────────────────────────
TMP=$(mktemp -d "${TMPDIR:-/tmp}/${BINARY_NAME}-install.XXXXXX")
cleanup() {
  rm -rf "$TMP" 2>/dev/null || true
  [ "$LOCKED" -eq 1 ] && rm -rf "$LOCK_DIR" 2>/dev/null || true
}
trap cleanup EXIT

# ─────────────────────────────────────────────────────────────────────────────
# Preflight checks: disk, write perms, existing install, network
# ─────────────────────────────────────────────────────────────────────────────
check_disk_space() {
  local min_kb=20480 path="$DEST"
  [ -d "$path" ] || path=$(dirname "$path")
  if command -v df >/dev/null 2>&1; then
    local avail_kb
    avail_kb=$(df -Pk "$path" 2>/dev/null | awk 'NR==2 {print $4}')
    if [ -n "$avail_kb" ] && [ "$avail_kb" -lt "$min_kb" ] 2>/dev/null; then
      err "Insufficient disk space in $path (need at least 20MB)"
      exit 1
    fi
  else
    warn "df not found; skipping disk space check"
  fi
}

check_write_permissions() {
  if [ ! -d "$DEST" ]; then
    if [ "$SYSTEM" -eq 1 ]; then
      sudo mkdir -p "$DEST" 2>/dev/null || { err "Cannot create $DEST (need sudo?)"; exit 1; }
    elif ! mkdir -p "$DEST" 2>/dev/null; then
      err "Cannot create $DEST (insufficient permissions)"
      err "Try --system (with sudo) or choose a writable --dest"
      exit 1
    fi
  fi
  if [ "$SYSTEM" -eq 0 ] && [ ! -w "$DEST" ]; then
    err "No write permission to $DEST"
    err "Try --system (with sudo) or choose a writable --dest"
    exit 1
  fi
}

check_existing_install() {
  local existing="$DEST/$BINARY_NAME"
  if [ -x "$existing" ]; then
    local cur
    cur=$("$existing" --version 2>/dev/null | head -1 || echo "")
    [ -n "$cur" ] && info "Existing install detected: $cur"
  fi
}

check_network() {
  [ "$FROM_SOURCE" -eq 1 ] && return 0
  command -v curl >/dev/null 2>&1 || { warn "curl not found; skipping network check"; return 0; }
  if ! xcurl -fsSL --connect-timeout 4 --max-time 8 -o /dev/null \
        "https://github.com/${OWNER}/${REPO}" 2>/dev/null; then
    warn "Network check to github.com failed; downloads may fail (proxy set?)"
  fi
}

preflight_checks() {
  info "Running preflight checks"
  check_disk_space
  check_write_permissions
  check_existing_install
  check_network
}
preflight_checks

# ─────────────────────────────────────────────────────────────────────────────
# Already-installed short-circuit (skip download/build; --force overrides)
# ─────────────────────────────────────────────────────────────────────────────
if [ "$FORCE" -eq 0 ] && [ -n "$VERSION" ] && [ -x "$DEST/$BINARY_NAME" ]; then
  CUR_VER=$("$DEST/$BINARY_NAME" --version 2>/dev/null | awk '{print $NF}' || echo "")
  if [ -n "$CUR_VER" ] && { [ "$CUR_VER" = "$VERSION_BARE" ] || [ "v$CUR_VER" = "$VERSION" ]; }; then
    ok "${BINARY_NAME} ${VERSION} is already installed at $DEST/$BINARY_NAME"
    info "Use --force to reinstall"
    exit 0
  fi
fi

# ─────────────────────────────────────────────────────────────────────────────
# Install helper
# ─────────────────────────────────────────────────────────────────────────────
install_binary() {
  local src_bin="$1"
  [ -x "$src_bin" ] || { err "Binary not found/executable: $src_bin"; exit 1; }
  if [ "$SYSTEM" -eq 1 ]; then
    sudo install -m 0755 "$src_bin" "$DEST/$BINARY_NAME"
  else
    install -m 0755 "$src_bin" "$DEST/$BINARY_NAME"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Build-from-source fallback.
#   cargo build --release --bin fmd   ->   target/release/fmd
# ─────────────────────────────────────────────────────────────────────────────
build_from_source() {
  if ! command -v cargo >/dev/null 2>&1; then
    err "cargo not found — building ${BINARY_NAME} from source needs the Rust toolchain."
    err "Install Rust via rustup, then re-run this installer:"
    err "  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y"
    err "  source \"\$HOME/.cargo/env\""
    err "More: https://rustup.rs"
    exit 1
  fi
  command -v git >/dev/null 2>&1 || { err "git not found — required to fetch the source"; exit 1; }

  local src src_is_clone=0
  if [ -f "./Cargo.toml" ] && grep -q '^name = "franken_markdown"' ./Cargo.toml 2>/dev/null; then
    src="$(pwd)"
    info "Building from local checkout: $src"
  else
    src="$TMP/src"; src_is_clone=1
    info "Cloning ${OWNER}/${REPO}…"
    if [ -n "$VERSION" ]; then
      if ! git clone --depth 1 --branch "$VERSION" \
            "https://github.com/${OWNER}/${REPO}.git" "$src" 2>/dev/null; then
        warn "Could not clone tag '$VERSION'; cloning the default branch instead"
        rm -rf "$src"
        git clone --depth 1 "https://github.com/${OWNER}/${REPO}.git" "$src"
      fi
    else
      git clone --depth 1 "https://github.com/${OWNER}/${REPO}.git" "$src"
    fi
  fi

  # The default `fmd` build never enables `batch`; if a source fallback lands on
  # an older tag with a non-portable optional Asupersync source, neutralize that
  # optional dep in our throwaway clone only. Never mutate the user's checkout.
  if [ "$src_is_clone" -eq 1 ] && [ ! -d "$src/../asupersync" ] && [ -f "$src/Cargo.toml" ]; then
    sed -i.bak \
      -e '/^asupersync[[:space:]]*=[[:space:]]*{/d' \
      -e 's/^batch[[:space:]]*=.*$/batch = ["cli"]/' \
      "$src/Cargo.toml"
    rm -f "$src/Cargo.toml.bak"
  fi

  info "Building ${BINARY_NAME} (cargo build --release --bin ${BINARY_NAME}) — this may take a few minutes"
  # Unset cargo target redirections so the binary lands at the expected path even
  # for developers who export CARGO_TARGET_DIR / CARGO_BUILD_TARGET.
  ( cd "$src" \
      && unset CARGO_TARGET_DIR CARGO_BUILD_TARGET_DIR CARGO_BUILD_TARGET \
      && cargo build --release --bin "$BINARY_NAME" )

  local built="$src/target/release/$BINARY_NAME"
  if [ ! -x "$built" ]; then
    built=$(find "$src/target" -maxdepth 4 -type f -name "$BINARY_NAME" -perm -111 2>/dev/null | head -n1)
  fi
  [ -n "$built" ] && [ -x "$built" ] || { err "Build finished but $BINARY_NAME was not found under target/"; exit 1; }

  install_binary "$built"
  BUILT_FROM_SOURCE=1
  ok "Installed to $DEST/$BINARY_NAME (built from source)"
}

# ─────────────────────────────────────────────────────────────────────────────
# Binary acquisition: 4-tier fallback ending in build-from-source
# ─────────────────────────────────────────────────────────────────────────────
download_one() {
  local url="$1" out="$2" label="$3"
  if [ "$HAS_GUM" -eq 0 ] || [ "$NO_GUM" -eq 1 ] || [ "$USE_COLOR" -eq 0 ] || [ "$QUIET" -eq 1 ] || [ ! -t 1 ]; then
    info "$label"
    xcurl -fsSL --connect-timeout 30 --max-time 1800 "$url" -o "$out"
  else
    printf '\033[1;36m↓\033[0m %s \033[2m%s\033[0m\n' "$label" "$(basename "$url")"
    xcurl -fL --progress-bar --connect-timeout 30 --max-time 1800 "$url" -o "$out"
  fi
}

DOWNLOADED_TAR=""
DOWNLOADED_URL=""
acquire_binary() {
  if [ "$FROM_SOURCE" -eq 1 ] || [ -z "$TARGET" ]; then
    build_from_source
    return 0
  fi

  # Build the ordered candidate list.
  local -a candidates=()
  if [ -n "$ARTIFACT_URL" ]; then
    candidates+=("$ARTIFACT_URL")
  fi
  if [ -n "$VERSION" ]; then
    # Tier 1: release workflow artifact under the tag. The archive name includes
    # the literal tag, e.g. fmd-v0.1.0-aarch64-apple-darwin.tar.gz.
    candidates+=("https://github.com/${OWNER}/${REPO}/releases/download/${VERSION}/${BINARY_NAME}-${VERSION}-${TARGET}.${EXT}")
    # Tier 2: compatibility names for any hand-uploaded assets.
    candidates+=("https://github.com/${OWNER}/${REPO}/releases/download/${VERSION}/${BINARY_NAME}-${VERSION_BARE}-${TARGET}.${EXT}")
    candidates+=("https://github.com/${OWNER}/${REPO}/releases/download/${VERSION}/${BINARY_NAME}-${TARGET}.${EXT}")
  fi
  # Tier 3: /releases/latest/download/ compatibility aliases.
  candidates+=("https://github.com/${OWNER}/${REPO}/releases/latest/download/${BINARY_NAME}-${TARGET}.${EXT}")
  candidates+=("https://github.com/${OWNER}/${REPO}/releases/latest/download/${BINARY_NAME}-${OS}-${ARCH}.${EXT}")

  local url tar
  for url in "${candidates[@]}"; do
    tar="$TMP/$(basename "$url")"
    if download_one "$url" "$tar" "Downloading ${BINARY_NAME} ${VERSION:-latest}"; then
      DOWNLOADED_TAR="$tar"
      DOWNLOADED_URL="$url"
      ok "Downloaded $(basename "$url")"
      return 0
    fi
    warn "Not available: $(basename "$url")"
  done

  # Tier 4: build from source.
  warn "No prebuilt binary available; falling back to build-from-source"
  build_from_source
}
acquire_binary

# If we built from source the binary is already installed — skip verify/extract.
if [ "$BUILT_FROM_SOURCE" -eq 0 ]; then
  # ───────────────────────────────────────────────────────────────────────────
  # Checksum verification (SHA256, dual tool)
  # ───────────────────────────────────────────────────────────────────────────
  TAR_BASENAME="$(basename "$DOWNLOADED_TAR")"
  if [ -z "$CHECKSUM" ]; then
    [ -n "$CHECKSUM_URL" ] || CHECKSUM_URL="https://github.com/${OWNER}/${REPO}/releases/download/${VERSION}/SHA256SUMS"
    info "Fetching checksums from ${CHECKSUM_URL}"
    if xcurl -fsSL --connect-timeout 30 --max-time 60 "$CHECKSUM_URL" -o "$TMP/SHA256SUMS" 2>/dev/null; then
      CHECKSUM=$(grep "  ${TAR_BASENAME}\$" "$TMP/SHA256SUMS" 2>/dev/null | awk '{print $1}')
      [ -n "$CHECKSUM" ] || CHECKSUM=$(grep " ${TAR_BASENAME}\$" "$TMP/SHA256SUMS" 2>/dev/null | awk '{print $1}')
    fi
    if [ -z "$CHECKSUM" ]; then
      # Sidecar fallback: <artifact>.sha256
      if xcurl -fsSL --connect-timeout 30 --max-time 60 "${DOWNLOADED_URL}.sha256" -o "$TMP/sidecar.sha256" 2>/dev/null; then
        CHECKSUM=$(awk 'NF>=1 && $1 ~ /^[0-9a-fA-F]{64}$/ {print $1; exit}' "$TMP/sidecar.sha256")
      fi
    fi
    if [ -z "$CHECKSUM" ]; then
      warn "Checksum for ${TAR_BASENAME} not found; skipping checksum verification"
      CHECKSUM="SKIP"
    fi
  fi

  if [ "$CHECKSUM" != "SKIP" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
      echo "$CHECKSUM  $DOWNLOADED_TAR" | sha256sum -c - >/dev/null 2>&1 \
        && ok "Checksum verified (sha256sum)" || { err "Checksum mismatch for ${TAR_BASENAME}"; exit 1; }
    elif command -v shasum >/dev/null 2>&1; then
      echo "$CHECKSUM  $DOWNLOADED_TAR" | shasum -a 256 -c - >/dev/null 2>&1 \
        && ok "Checksum verified (shasum)" || { err "Checksum mismatch for ${TAR_BASENAME}"; exit 1; }
    else
      warn "Neither sha256sum nor shasum found; skipping checksum verification"
    fi
  fi

  # ───────────────────────────────────────────────────────────────────────────
  # Sigstore / cosign verification (best-effort; soft-skip if absent)
  # ───────────────────────────────────────────────────────────────────────────
  if command -v cosign >/dev/null 2>&1; then
    bundle_url="${SIGSTORE_BUNDLE_URL:-${DOWNLOADED_URL}.sigstore.json}"
    if xcurl -fsSL --connect-timeout 30 --max-time 60 "$bundle_url" -o "$TMP/bundle.sigstore.json" 2>/dev/null; then
      if cosign verify-blob \
            --bundle "$TMP/bundle.sigstore.json" \
            --certificate-identity-regexp "$COSIGN_IDENTITY_RE" \
            --certificate-oidc-issuer "$COSIGN_OIDC_ISSUER" \
            "$DOWNLOADED_TAR" >/dev/null 2>&1; then
        ok "Signature verified (cosign)"
      else
        err "cosign signature verification FAILED for ${TAR_BASENAME}"
        exit 1
      fi
    else
      warn "Sigstore bundle not published; skipping signature verification"
    fi
  else
    warn "cosign not found; skipping signature verification (install cosign for authenticity checks)"
  fi

  # ───────────────────────────────────────────────────────────────────────────
  # Extract + install
  # ───────────────────────────────────────────────────────────────────────────
  info "Extracting"
  case "$TAR_BASENAME" in
    *.tar.gz|*.tgz) tar -xzf "$DOWNLOADED_TAR" -C "$TMP" ;;
    *.tar.xz)       tar -xJf "$DOWNLOADED_TAR" -C "$TMP" ;;
    *.zip)          unzip -qo "$DOWNLOADED_TAR" -d "$TMP" ;;
    *)              err "Unknown archive format: $TAR_BASENAME"; exit 1 ;;
  esac

  BIN="$TMP/$BINARY_NAME"
  if [ ! -x "$BIN" ]; then
    BIN=$(find "$TMP" -maxdepth 3 -type f -name "$BINARY_NAME" -perm -111 2>/dev/null | head -n1)
  fi
  [ -n "$BIN" ] && [ -x "$BIN" ] || { err "Binary '$BINARY_NAME' not found inside the archive"; exit 1; }

  install_binary "$BIN"
  ok "Installed to $DEST/$BINARY_NAME"
fi

# ─────────────────────────────────────────────────────────────────────────────
# PATH setup
# ─────────────────────────────────────────────────────────────────────────────
PATH_NOTE=""
maybe_add_path() {
  case ":$PATH:" in
    *:"$DEST":*) return 0 ;;
    *)
      if [ "$EASY" -eq 1 ]; then
        local updated=0 rc
        for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
          if [ -e "$rc" ] && [ -w "$rc" ]; then
            if ! grep -qF "$DEST" "$rc" 2>/dev/null; then
              # shellcheck disable=SC2016
              printf '\nexport PATH="%s:$PATH"\n' "$DEST" >> "$rc"
            fi
            updated=1
          fi
        done
        if [ "$updated" -eq 1 ]; then
          PATH_NOTE="PATH updated in shell rc; restart your shell to use ${BINARY_NAME}"
          warn "$PATH_NOTE"
        else
          PATH_NOTE="Add $DEST to your PATH to use ${BINARY_NAME}"
          warn "$PATH_NOTE"
        fi
      else
        PATH_NOTE="Add $DEST to your PATH to use ${BINARY_NAME} (or re-run with --easy-mode)"
        warn "$PATH_NOTE"
      fi
    ;;
  esac
}
maybe_add_path

# Shell completions: fmd does not (yet) expose a `completions <shell>` subcommand,
# so there is nothing to install. This is intentionally a clean no-op; revisit if
# a completions generator is added to the CLI.

# ─────────────────────────────────────────────────────────────────────────────
# --verify self-test
# ─────────────────────────────────────────────────────────────────────────────
if [ "$VERIFY" -eq 1 ]; then
  BINPATH="$DEST/$BINARY_NAME"
  if ! VER_OUT=$("$BINPATH" --version 2>&1); then
    err "Self-test failed: '$BINPATH --version' did not run"
    err "$VER_OUT"
    exit 1
  fi
  ok "Self-test: $VER_OUT"

  # Render smoke test (best-effort): Markdown -> HTML to stdout.
  if SMOKE=$("$BINPATH" --text '# fmd smoke test' --out - 2>/dev/null) \
       && printf '%s' "$SMOKE" | grep -qi 'smoke test'; then
    ok "Render smoke test passed (Markdown → HTML)"
  else
    warn "Render smoke test did not produce expected HTML (binary still installed)"
  fi

  # Doctor (best-effort health report).
  if "$BINPATH" doctor >/dev/null 2>&1; then
    ok "fmd doctor reports healthy"
  fi
fi

# ─────────────────────────────────────────────────────────────────────────────
# Final summary
# ─────────────────────────────────────────────────────────────────────────────
SRC_NOTE="prebuilt binary"
[ "$BUILT_FROM_SOURCE" -eq 1 ] && SRC_NOTE="built from source"
DISPLAY_VERSION="${VERSION:-$( "$DEST/$BINARY_NAME" --version 2>/dev/null | awk '{print $NF}' )}"
[ -n "$DISPLAY_VERSION" ] || DISPLAY_VERSION="(source build)"

if [ "$QUIET" -eq 0 ]; then
  if [ "$HAS_GUM" -eq 1 ] && [ "$NO_GUM" -eq 0 ]; then
    echo ""
    gum style \
      --border rounded --border-foreground 42 \
      --padding "0 2" --margin "0" \
      "$(gum style --foreground 42 --bold '✓ fmd installed!')" \
      "" \
      "$(gum style --foreground 245 "Binary:   $(gum style --bold "$DEST/$BINARY_NAME")")" \
      "$(gum style --foreground 245 "Version:  $(gum style --bold "$DISPLAY_VERSION")  ($SRC_NOTE)")" \
      "" \
      "$(gum style --foreground 39 --bold 'Quick start:')" \
      "$(gum style --foreground 245 '  fmd README.md --out README.html')" \
      "$(gum style --foreground 245 '  fmd README.md --to pdf --out README.pdf')" \
      "$(gum style --foreground 245 "  fmd --text '# Hello' --out -")" \
      "$(gum style --foreground 245 '  fmd capabilities --json')" \
      "" \
      "$(gum style --foreground 245 "Uninstall: rm -f $DEST/$BINARY_NAME")"
    echo ""
  else
    echo ""
    if [ "$USE_COLOR" -eq 1 ]; then
      draw_box "1;32" \
        "$(printf '\033[1;32m✓ fmd installed!\033[0m')" \
        "" \
        "Binary:   \033[1m$DEST/$BINARY_NAME\033[0m" \
        "Version:  \033[1m$DISPLAY_VERSION\033[0m  ($SRC_NOTE)" \
        "" \
        "\033[1;36mQuick start:\033[0m" \
        "  \033[0;90mfmd README.md --out README.html\033[0m" \
        "  \033[0;90mfmd README.md --to pdf --out README.pdf\033[0m" \
        "  \033[0;90mfmd --text '# Hello' --out -\033[0m" \
        "  \033[0;90mfmd capabilities --json\033[0m" \
        "" \
        "Uninstall: \033[0;90mrm -f $DEST/$BINARY_NAME\033[0m"
    else
      draw_box "1;32" \
        "✓ fmd installed!" \
        "" \
        "Binary:   $DEST/$BINARY_NAME" \
        "Version:  $DISPLAY_VERSION  ($SRC_NOTE)" \
        "" \
        "Quick start:" \
        "  fmd README.md --out README.html" \
        "  fmd README.md --to pdf --out README.pdf" \
        "  fmd --text '# Hello' --out -" \
        "  fmd capabilities --json" \
        "" \
        "Uninstall: rm -f $DEST/$BINARY_NAME"
    fi
    echo ""
  fi
fi

exit 0
