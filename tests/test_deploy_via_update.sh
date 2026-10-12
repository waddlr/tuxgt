#!/usr/bin/env bash
# make deploy end to end: seed an older prefix (stale binary, stale
# lib/share files, one dropped official + payload, one manifest-stale
# owned file, user data), run `make deploy` against a temp HOME, and
# prove the prepared tree lands via the staged binary's
# `update --from` (binary hash, drop-gone, manifest tag) with user
# data intact. The stale bin/tuxgt is deliberately un-runnable:
# passing proves deploy never executes the old installed bytes.
#
# Hermetic: temp HOME, temp prefix — never the real HOME. Needs a
# prepared tree: `make prepare`, or TUXGT_TEST_PKG pointing at a
# `tuxgt/` tree with a real bin/tuxgt. `make deploy` rebuilds the
# tree itself; the seeded copy is what gets aged.
#
# Run from the repo root: `bash tests/test_deploy_via_update.sh`
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
# Pin BUILD so the prepared-tree and tarball paths below are deterministic.
export BUILD=build
pkg="${TUXGT_TEST_PKG:-$root/build/pkg/tuxgt}"
[ "$(basename -- "$pkg")" = "tuxgt" ] || { echo "FAIL: TUXGT_TEST_PKG must be a tuxgt/ tree (got $pkg)" >&2; exit 1; }
[ -x "$pkg/bin/tuxgt" ] || { echo "FAIL: no prepared tree at $pkg (run 'make prepare' or set TUXGT_TEST_PKG)" >&2; exit 1; }

# Fast routing check: deploy must go through the staged binary's
# `update --from`, not a hand-rolled overlay.
n_deploy=$(make -C "$root" -n -B deploy 2>/dev/null)
printf '%s\n' "$n_deploy" | grep -q 'update --from' || { echo "FAIL: make deploy does not route through update --from" >&2; exit 1; }
printf '%s\n' "$n_deploy" | grep -qF '"build/pkg/tuxgt/bin/tuxgt" update --from' || { echo "FAIL: make deploy does not invoke the staged binary's update --from" >&2; exit 1; }

ver=$(grep -E '^version = "[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?"$' "$root/src/tuxgt/Cargo.toml" | head -1 | cut -d'"' -f2)
[ -n "$ver" ] || { echo "FAIL: could not parse workspace version" >&2; exit 1; }

work=$(mktemp -d /tmp/tuxgt-deploy-test.XXXXXX)
if command -v realpath >/dev/null 2>&1; then work="$(realpath -m "$work")"; fi
trap 'rm -rf "$work"' EXIT INT TERM
home="$work/home"; mkdir -p "$home"
dest="$work/prefix"

# Old prefix: the prepared tree, aged.
cp -a "$pkg" "$dest"
printf 'stale-binary tuxgt/9.9.8\n' > "$dest/bin/tuxgt"
chmod +x "$dest/bin/tuxgt"
printf 'stale\n' > "$dest/bin/old-tool"
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
# Previous manifest: update --from drops owned files missing from
# the new set (bin/old-tool, the stale official + its payload).
mkdir -p "$dest/config"
{
    printf 'tag = "v0.0.1"\n'
    printf 'files = [\n'
    printf '  { path = "bin/tuxgt", sha256 = "prev" },\n'
    printf '  { path = "bin/old-tool", sha256 = "prev" },\n'
    printf '  { path = "mods/official/reshade.toml", sha256 = "prev" },\n'
    printf '  { path = "mods/official/zz-stale.toml", sha256 = "prev" },\n'
    printf ']\n'
} > "$dest/config/prefix-manifest.toml"

mkdir -p "$home/.config"
printf '# tuxgt boot\nTUXGT_DATA=%s\n' "$dest" > "$home/.config/tuxgt.conf"

# `make deploy` rebuilds via cargo; rustup finds its toolchain under
# the real HOME, so pin RUSTUP_HOME/CARGO_HOME before faking HOME.
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"

out="$work/out.txt"
rc=0
env HOME="$home" RUSTUP_HOME="$rustup_home" CARGO_HOME="$cargo_home" make -C "$root" deploy >"$out" 2>&1 || rc=$?

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
t "deploy reports update --from" grep -q "deployed to $dest via update --from" "$out"
t "staged update ran (updated line)" grep -q "^updated" "$out"
t "manifest records the workspace tag" grep -q "v$ver" "$dest/config/prefix-manifest.toml"
t "new binary lands" cmp "$dest/bin/tuxgt" "$pkg/bin/tuxgt"
t "lib refreshed" cmp "$dest/lib/libtuxgt-launcher.so" "$pkg/lib/libtuxgt-launcher.so"
t "share refreshed" cmp "$dest/share/templates/custom-blank.toml" "$pkg/share/templates/custom-blank.toml"
t "manifest-stale owned file gone" test "!" -e "$dest/bin/old-tool"
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
t "deploy tarball cleaned up" test "!" -e "$root/build/deploy.tar.gz"

if [ "$fail" -ne 0 ]; then echo "--- make deploy output ---"; cat "$out"; exit 1; fi
echo "PASS: make deploy applies the prepared tree via update --from"
