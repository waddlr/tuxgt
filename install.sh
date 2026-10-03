#!/usr/bin/env bash
# TuxGT quick installer: welcome, ask prefix, download the latest release
# onto that filesystem, unpack, and run packaged `tuxgt install`.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/waddlr/tuxgt/master/install.sh | bash
#
# Env overrides:
#   TUXGT_RELEASE_URL  full URL of the release tarball (default below)
#   TUXGT_PREFIX       install directory (skips the prefix prompt)
#   TUXGT_LATEST       target version (skips GitHub latest lookup; tests)
set -eu

RELEASE_URL="${TUXGT_RELEASE_URL:-https://github.com/waddlr/tuxgt/releases/latest/download/tuxgt.tar.gz}"
ASSET="tuxgt.tar.gz"

# `curl | bash` puts the script on stdin; stdout is still the terminal.
# Print immediately — do not wait on the tarball fetch.
say() { printf '%s\n' "$*"; }

# Read a line from the user's terminal (not the curl pipe). Fails when
# no tty is attached (CI, non-interactive).
ask() {
    if { exec 3</dev/tty; } 2>/dev/null; then
        printf '%s' "$1" >/dev/tty 2>/dev/null || printf '%s' "$1"
        read -r reply <&3 || reply=""
        exec 3<&-
        return 0
    fi
    return 1
}

say "TuxGT — Linux-native manager for injector/runtime mods."
say ""
say "This installer sets up the latest release. No sudo. Game folders stay where they are."
say ""

command -v tar >/dev/null 2>&1 || { echo "error: need 'tar' installed" >&2; exit 1; }

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fL --progress-bar -o "$1" "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -q --show-progress -O "$1" "$2"; }
else
    echo "error: need 'curl' or 'wget' installed" >&2; exit 1
fi

home="${HOME:?error: HOME is not set}"
tmpdir="${TMPDIR:-/tmp}"

# Quote the strip pattern: unquoted #~/ tilde-expands to $HOME/ and
# turns ~/tuxgt into $HOME/~/tuxgt.
expand_tilde() {
    case "$1" in
        "~") printf '%s\n' "$home" ;;
        "~/"*) printf '%s\n' "$home/${1#"~/"}" ;;
        *) printf '%s\n' "$1" ;;
    esac
}

# Print "M m p pre" for vX.Y.Z / X.Y.Z / X.Y.Z-beta.N.
# GNU sort -V ranks 0.5.1 < 0.5.1-beta.1; a final must beat its beta.
norm_ver() {
    local v="${1#v}"
    if [[ "$v" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
        printf '%d %d %d %d\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[3]}" 1000000
        return 0
    fi
    if [[ "$v" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)-beta\.([0-9]+)$ ]]; then
        printf '%d %d %d %d\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[3]}" "${BASH_REMATCH[4]}"
        return 0
    fi
    return 1
}

# True when $1 < $2. False on equal, greater, or unparseable.
ver_lt() {
    local a b a1 a2 a3 a4 b1 b2 b3 b4
    a="$(norm_ver "$1")" || return 1
    b="$(norm_ver "$2")" || return 1
    # shellcheck disable=SC2086
    set -- $a
    a1=$1 a2=$2 a3=$3 a4=$4
    # shellcheck disable=SC2086
    set -- $b
    b1=$1 b2=$2 b3=$3 b4=$4
    [ "$a1" -lt "$b1" ] && return 0
    [ "$a1" -gt "$b1" ] && return 1
    [ "$a2" -lt "$b2" ] && return 0
    [ "$a2" -gt "$b2" ] && return 1
    [ "$a3" -lt "$b3" ] && return 0
    [ "$a3" -gt "$b3" ] && return 1
    [ "$a4" -lt "$b4" ]
}

installed_ver() {
    local hits
    hits="$(grep -aoE 'tuxgt/[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?' "$1" 2>/dev/null | sed 's|^tuxgt/||' | sort -u)" || true
    [ -n "$hits" ] || return 1
    [ "$(printf '%s\n' "$hits" | wc -l)" -eq 1 ] || return 1
    printf '%s\n' "$hits"
}

# TUXGT_LATEST, or a version pinned in RELEASE_URL, or GitHub /releases/latest.
resolve_latest() {
    local v loc
    if [ -n "${TUXGT_LATEST:-}" ]; then
        printf '%s\n' "${TUXGT_LATEST#v}"
        return 0
    fi
    case "$RELEASE_URL" in
        */releases/download/v[0-9]*)
            v="${RELEASE_URL##*/releases/download/}"
            v="${v%%/*}"
            printf '%s\n' "${v#v}"
            return 0
            ;;
    esac
    loc=""
    if command -v curl >/dev/null 2>&1; then
        loc="$(curl -sS --max-time 15 -o /dev/null -w '%{redirect_url}' https://github.com/waddlr/tuxgt/releases/latest </dev/null)" || loc=""
    elif command -v wget >/dev/null 2>&1; then
        loc="$(wget -qS --timeout=15 --spider https://github.com/waddlr/tuxgt/releases/latest </dev/null 2>&1 | tr -d '\r' | awk 'BEGIN{IGNORECASE=1} /^  Location:/{print $2; exit}')" || loc=""
    fi
    case "$loc" in
        */tag/v*)
            v="${loc##*/tag/}"
            v="${v#v}"
            v="${v%%[\?#]*}"
            v="${v%/}"
            printf '%s\n' "$v"
            return 0
            ;;
    esac
    return 1
}

# Conf TUXGT_DATA is the default only for a durable existing install.
# Skip /tmp stages (EXDEV leftovers) and a literal /~/ from a bad expand.
usable_prefix() {
    case "$1" in
        "" | "$tmpdir"/* | /tmp/* | *"/~/"*) return 1 ;;
    esac
    [ -f "$1/bin/tuxgt" ]
}

default="$home/tuxgt"
if [ -f "$home/.config/tuxgt.conf" ]; then
    existing="$(grep -E '^TUXGT_DATA=' "$home/.config/tuxgt.conf" | head -1 | cut -d= -f2- | tr -d '\042\047')"
    existing="$(expand_tilde "$existing")"
    if usable_prefix "$existing"; then
        default="$existing"
    fi
fi

if [ -n "${TUXGT_PREFIX:-}" ]; then
    dest="$(expand_tilde "$TUXGT_PREFIX")"
elif usable_prefix "$default"; then
    dest="$default"
    say "Using existing install at $dest"
else
    if ask "Install prefix [${default}]: "; then
        if [ -z "$reply" ]; then
            dest="$default"
        else
            dest="$(expand_tilde "$reply")"
        fi
    else
        dest="$default"
        say "Using $dest"
    fi
fi

case "$dest" in
    /*) ;;
    *) dest="$(pwd)/$dest" ;;
esac
if command -v realpath >/dev/null 2>&1; then
    dest="$(realpath -m "$dest")"
fi

parent="$(dirname -- "$dest")"
mkdir -p "$parent"

if [ -e "$dest" ] && [ ! -f "$dest/bin/tuxgt" ]; then
    echo "error: refusing to clobber $dest" >&2
    exit 1
fi

# Default Yes when there is no tty (CI / TUXGT_PREFIX tests).
confirm() {
    if ask "$1"; then
        case "$reply" in
            [nN]*) return 1 ;;
        esac
    fi
    return 0
}

need_fetch=1
if [ -f "$dest/bin/tuxgt" ]; then
    have="$(installed_ver "$dest/bin/tuxgt" || true)"
    latest="$(resolve_latest || true)"
    if [ -n "${have:-}" ] && [ -n "${latest:-}" ] && norm_ver "$latest" >/dev/null; then
        if ver_lt "$have" "$latest"; then
            say "TuxGT v$have at $dest. Latest is v$latest."
            if ! confirm "Update to v$latest? [Y/n] "; then
                say "Update cancelled."
                exit 0
            fi
        elif ver_lt "$latest" "$have"; then
            say "Installed v$have is newer than latest v$latest — leaving it."
            need_fetch=0
        else
            say "Already up to date (v$have)."
            need_fetch=0
        fi
    else
        say "Existing install at $dest."
        if ! confirm "Update it to the latest release? [Y/n] "; then
            say "Update cancelled."
            exit 0
        fi
    fi
fi

if [ "$need_fetch" -eq 1 ]; then
    # Stage on the dest filesystem so `mv` is not EXDEV (/tmp is often tmpfs).
    stage="$(mktemp -d -p "$parent" .tuxgt-install.XXXXXX)"
    cleanup() { rm -rf "$stage"; }
    trap cleanup EXIT INT TERM

    say "Downloading $ASSET …"
    fetch "$stage/$ASSET" "$RELEASE_URL"

    say "Unpacking …"
    tar -xzf "$stage/$ASSET" -C "$stage"
    rm -f "$stage/$ASSET"
    src="$stage/tuxgt"
    [ -x "$src/bin/tuxgt" ] || { echo "error: $src/bin/tuxgt missing from $ASSET" >&2; exit 1; }

    if [ ! -e "$dest" ]; then
        if ! mv "$src" "$dest"; then
            cp -a "$src" "$dest"
            rm -rf "$src"
        fi
    else
        cp -a "$src/." "$dest/"
    fi
fi

say "Writing host files …"
"$dest/bin/tuxgt" install --prefix "$dest" --yes

app="$dest/bin/tuxgt"
[ -x "$app" ] || { echo "error: $app missing after install" >&2; exit 1; }

if ask "Launch TuxGT now? [Y/n] "; then
    case "$reply" in
        [nN]*) say "Installed. Run '$app' or find TuxGT in your app menu." ;;
        *)
            nohup "$app" >/dev/null 2>&1 &
            disown 2>/dev/null || true
            say "Launched (detached). Find TuxGT in your app menu under Games."
            ;;
    esac
else
    say "Installed. Run '$app' or find TuxGT in your app menu."
fi
