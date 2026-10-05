#!/usr/bin/env bash
# install.sh update-case end to end: seed an older prefix (stale binary,
# stale lib/share files, one dropped official + payload, user files),
# serve a local staged release over file://, run install.sh with no tty,
# and prove the new tree lands (binary hash, drop-gone) with user data
# intact. The stale bin/tuxgt is deliberately un-runnable: passing proves
# the update never executes the installed tree (SIGILL recovery).
#
# Hermetic: temp HOME, temp prefix, file:// release URL — never the real
# HOME, never network GitHub (TUXGT_LATEST pins the version lookup).
# Needs a prepared tree: `make prepare`, or TUXGT_TEST_PKG pointing at a
# `tuxgt/` tree with a real bin/tuxgt.
#
# Run from the repo root: `bash tests/test_install_sh_update.sh`
set -eu

command -v setsid >/dev/null 2>&1 || { echo "FAIL: need 'setsid' (util-linux) for the no-tty run" >&2; exit 1; }

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
pkg="${TUXGT_TEST_PKG:-$root/build/pkg/tuxgt}"
[ "$(basename -- "$pkg")" = "tuxgt" ] || { echo "FAIL: TUXGT_TEST_PKG must be a tuxgt/ tree (got $pkg)" >&2; exit 1; }
[ -x "$pkg/bin/tuxgt" ] || { echo "FAIL: no prepared tree at $pkg (run 'make prepare' or set TUXGT_TEST_PKG)" >&2; exit 1; }

work=$(mktemp -d /tmp/tuxgt-install-sh-test.XXXXXX)
if command -v realpath >/dev/null 2>&1; then work="$(realpath -m "$work")"; fi
trap 'rm -rf "$work"' EXIT INT TERM
home="$work/home"; mkdir -p "$home"
dest="$work/prefix"

# Old prefix: the prepared tree, aged. Stale 9.9.8 sits below the pinned
# TUXGT_LATEST=9.9.9 so the version guard always takes the update path.
cp -a "$pkg" "$dest"
printf 'stale-binary tuxgt/9.9.8\n' > "$dest/bin/tuxgt"
chmod +x "$dest/bin/tuxgt"
printf 'stale-so\n' > "$dest/lib/libtuxgt-launcher.so"
printf 'stale-template\n' > "$dest/share/templates/custom-blank.toml"
printf '# stale official\n' > "$dest/mods/official/zz-stale.toml"
mkdir -p "$dest/mods/official/zz-stale"
printf 'stale-payload\n' > "$dest/mods/official/zz-stale/blob.bin"
mkdir -p "$dest/mods/official/reshade"
printf 'kept-payload\n' > "$dest/mods/official/reshade/blob.bin"
mkdir -p "$dest/games/g1" "$dest/downloads" "$dest/config" "$dest/mods/user"
printf 'save\n' > "$dest/games/g1/save.dat"
printf 'part\n' > "$dest/downloads/d.part"
printf 'db\n' > "$dest/config/tuxgt.sqlite"
printf 'mine\n' > "$dest/mods/user/mine.toml"

# Local staged release: tar the prepared tree, serve over file://.
rel="$work/rel"; mkdir -p "$rel"
tar -czf "$rel/tuxgt.tar.gz" -C "$(dirname -- "$pkg")" tuxgt

# No tty (setsid) + stdin /dev/null: default-Yes confirms, no launch.
out="$work/out.txt"
rc=0
setsid -w env HOME="$home" TUXGT_PREFIX="$dest" \
    TUXGT_RELEASE_URL="file://$rel/tuxgt.tar.gz" TUXGT_LATEST="9.9.9" \
    bash "$root/install.sh" </dev/null >"$out" 2>&1 || rc=$?

fail=0
t() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then echo "ok: $desc"; else echo "FAIL: $desc"; fail=1; fi
}

printf 'kept-payload\n' > "$work/exp-payload"
printf 'save\n' > "$work/exp-save"
printf 'part\n' > "$work/exp-part"
printf 'db\n' > "$work/exp-db"
printf 'mine\n' > "$work/exp-mine"

t "exit 0" test "$rc" -eq 0
t "version guard takes update" grep -q "Latest is v9.9.9" "$out"
t "staged-tree install ran (overlaid line)" grep -q "overlaid" "$out"
t "new binary lands" cmp "$dest/bin/tuxgt" "$pkg/bin/tuxgt"
t "lib refreshed" cmp "$dest/lib/libtuxgt-launcher.so" "$pkg/lib/libtuxgt-launcher.so"
t "share refreshed" cmp "$dest/share/templates/custom-blank.toml" "$pkg/share/templates/custom-blank.toml"
t "dropped official toml gone" test "!" -e "$dest/mods/official/zz-stale.toml"
t "dropped payload dir gone" test "!" -e "$dest/mods/official/zz-stale"
t "kept official present" test -f "$dest/mods/official/reshade.toml"
t "kept payload intact" cmp "$dest/mods/official/reshade/blob.bin" "$work/exp-payload"
t "games/ intact" cmp "$dest/games/g1/save.dat" "$work/exp-save"
t "downloads/ intact" cmp "$dest/downloads/d.part" "$work/exp-part"
t "config/ intact" cmp "$dest/config/tuxgt.sqlite" "$work/exp-db"
t "mods/user intact" cmp "$dest/mods/user/mine.toml" "$work/exp-mine"
t "conf points at prefix" grep -qF "TUXGT_DATA=$dest" "$home/.config/tuxgt.conf"
t "bin link points at prefix" test "$(readlink "$home/.local/bin/tuxgt")" = "$dest/bin/tuxgt"
t "no-tty completion" grep -q "Installed. Run" "$out"

if [ "$fail" -ne 0 ]; then echo "--- install.sh output ---"; cat "$out"; exit 1; fi
echo "PASS: install.sh update case overlays the new tree"
