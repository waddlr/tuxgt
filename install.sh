#!/usr/bin/env bash
# TuxGT quick installer: downloads the latest release tarball, unpacks it to a
# temp dir, and runs the packaged `tuxgt install` (which asks where to put it).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/waddlr/tuxgt/master/install.sh | bash
#
# Env overrides:
#   TUXGT_RELEASE_URL  full URL of the release tarball (default below)
set -eu

RELEASE_URL="${TUXGT_RELEASE_URL:-https://github.com/waddlr/tuxgt/releases/latest/download/tuxgt.tar.gz}"
ASSET="tuxgt.tar.gz"

command -v tar >/dev/null 2>&1 || { echo "error: need 'tar' installed" >&2; exit 1; }

stage="$(mktemp -d /tmp/tuxgt-install.XXXXXXXX)"
cleanup() {
    # `tuxgt install` moves the tree to the chosen prefix — but across
    # filesystems it stays here, so only wipe the stage dir when the install
    # landed elsewhere.
    case "${PREFIX_DIR:-}" in
        "$stage"/*) echo "note: install stayed in $stage (could not move across filesystems); leaving it in place" >&2 ;;
        *) rm -rf "$stage" ;;
    esac
}
trap cleanup EXIT INT TERM

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$RELEASE_URL" -o "$stage/$ASSET"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$stage/$ASSET" "$RELEASE_URL"
else
    echo "error: need 'curl' or 'wget' installed" >&2; exit 1
fi

tar -xzf "$stage/$ASSET" -C "$stage"
bin="$stage/tuxgt/bin/tuxgt"
[ -x "$bin" ] || { echo "error: $bin missing from $ASSET" >&2; exit 1; }

# Under `curl | bash` stdin is the pipe, not the terminal: give the installer
# the terminal so the prefix prompt works.
"$bin" install </dev/tty

PREFIX_DIR="$(grep -E '^TUXGT_DATA=' "$HOME/.config/tuxgt.conf" 2>/dev/null | head -1 | cut -d= -f2- | tr -d '\042\047')"
[ -n "${PREFIX_DIR:-}" ] || { echo "error: install finished but $HOME/.config/tuxgt.conf has no TUXGT_DATA" >&2; exit 1; }
app="$PREFIX_DIR/bin/tuxgt"
[ -x "$app" ] || { echo "error: $app missing after install" >&2; exit 1; }

printf 'Launch TuxGT now? [Y/n] '
reply=""; read -r reply </dev/tty || true
case "$reply" in
    [nN]*) echo "Installed. Run '$app' or find TuxGT in your app menu." ;;
    *)
        nohup "$app" >/dev/null 2>&1 &
        disown 2>/dev/null || true
        echo "Launched (detached). Find TuxGT in your app menu under Games." ;;
esac
