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
say "This installer downloads the latest release into a directory you choose."
say "No sudo. Game folders stay where they are."
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
elif ask "Install prefix [${default}]: "; then
    if [ -z "$reply" ]; then
        dest="$default"
    else
        dest="$(expand_tilde "$reply")"
    fi
else
    dest="$default"
    say "Using $dest"
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
if [ -f "$dest/bin/tuxgt" ]; then
    say "Existing install at $dest — this will update it."
fi

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
