#!/usr/bin/env bash
# Reproducible-build regression: C -march pins (pin-c-baseline), proxy PE
# timestamp (proxy-no-pe-timestamp), SDE wiring + embedded instant
# (deterministic-build-date), byte-identical tarballs (reproducible-tarball).
#
# Fast section first (dry-run flag checks via `make -n -B`, stub-cargo probes of
# the release SDE export — nothing built), then end-to-end proofs (two
# time-separated same-SDE packages → identical sha256; proxy DLL TimeDateStamp
# is zero; the embedded instant is the SDE instant).
#
# Needs: gcc, cargo, GNU tar, gzip. mingw + objdump only for the PE stamp
# section (skipped without them). Runs `make package` (dev) three times plus
# `make build`/`make proxy` — comparable to test_install_sh_update.sh's
# prepared tree.
#
# Run from the repo root: `bash tests/test_reproducible_build.sh`
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
for c in gcc cargo tar gzip sha256sum; do
    command -v "$c" >/dev/null 2>&1 || { echo "FAIL: need '$c' installed" >&2; exit 1; }
done

fail=0
t() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then echo "ok: $desc"; else echo "FAIL: $desc"; fail=1; fi
}

cd "$root"

work=$(mktemp -d /tmp/tuxgt-repro-test.XXXXXX)
trap 'rm -rf "$work"' EXIT INT TERM
trap 'echo "--- make logs ---"; tail -n 8 "$work"/pkg*.log "$work"/build.log "$work"/proxy.log 2>/dev/null' ERR

# --- fast flag checks (dry run, nothing built) ---
n_preload=$(make -n -B preload 2>/dev/null)
n_proxy=$(make -n -B proxy 2>/dev/null)
n_package=$(make -n -B package 2>/dev/null)

t "preload pins baseline on both lines" sh -c \
    'test "$(printf "%s\n" "$1" | grep -c -- "-march=x86-64 -mtune=generic")" -ge 2' sh "$n_preload"
t "proxy pins baseline" sh -c \
    'printf "%s\n" "$1" | grep -q -- "-march=x86-64 -mtune=generic"' sh "$n_proxy"
t "proxy drops PE timestamp" sh -c \
    'printf "%s\n" "$1" | grep -q -- "-Wl,--no-insert-timestamp"' sh "$n_proxy"
t "tarball sorts entries" sh -c \
    'printf "%s\n" "$1" | grep -q -- "--sort=name"' sh "$n_package"
t "tarball zeroes ownership" sh -c \
    'printf "%s\n" "$1" | grep -q -- "--owner=0 --group=0"' sh "$n_package"
t "tarball pins mtime to SDE" sh -c \
    'printf "%s\n" "$1" | grep -Eq -- "--mtime=@[0-9]+"' sh "$n_package"
t "gzip strips name/timestamp" sh -c \
    'printf "%s\n" "$1" | grep -q -- "gzip -n"' sh "$n_package"

# --- SDE export wiring (stub cargo prints what make hands it) ---
mkdir -p "$work/bin"
printf '#!/usr/bin/env bash\necho "STUB_SDE=${SOURCE_DATE_EPOCH-unset}"\n' >"$work/bin/cargo"
chmod +x "$work/bin/cargo"

sde_release=$(env -u SOURCE_DATE_EPOCH PATH="$work/bin:$PATH" make build TYPE=release -s 2>/dev/null)
t "release exports SDE to cargo" sh -c \
    'printf "%s" "$1" | grep -Eq "STUB_SDE=[0-9]+"' sh "$sde_release"
sde_dev=$(env -u SOURCE_DATE_EPOCH PATH="$work/bin:$PATH" make build -s 2>/dev/null)
t "dev does not export SDE" sh -c \
    'test "$1" = "STUB_SDE=unset"' sh "$sde_dev"
sde_env=$(SOURCE_DATE_EPOCH=42 PATH="$work/bin:$PATH" make build TYPE=release -s 2>/dev/null)
t "env SDE wins" sh -c 'test "$1" = "STUB_SDE=42"' sh "$sde_env"

# --- end to end: same SDE, time-separated prepares → identical bytes ---
SOURCE_DATE_EPOCH=1671809360 make package >"$work/pkg1.log" 2>&1
sha1=$(sha256sum dist/tuxgt.tar.gz | cut -d' ' -f1)
sleep 1.1 # cross a wall-clock second so equal bytes prove normalization
SOURCE_DATE_EPOCH=1671809360 make package >"$work/pkg2.log" 2>&1
sha2=$(sha256sum dist/tuxgt.tar.gz | cut -d' ' -f1)
t "same SDE packages byte-identical" test "$sha1" = "$sha2"
SOURCE_DATE_EPOCH=7 make package >"$work/pkg3.log" 2>&1
sha3=$(sha256sum dist/tuxgt.tar.gz | cut -d' ' -f1)
t "SDE drives tarball bytes" test "$sha1" != "$sha3"

# --- end to end: the embedded instant is the SDE instant ---
SOURCE_DATE_EPOCH=1671809360 make build >"$work/build.log" 2>&1
t "binary embeds SDE instant" grep -rq "2022-12-23T15:29:20" src/tuxgt/target/debug/build/tuxgt-*/output

# --- end to end: proxy PE stamp (needs mingw; skipped without it) ---
if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 && command -v objdump >/dev/null 2>&1; then
    rm -f build/nvngx_dlssnr.dll
    make proxy >"$work/proxy.log" 2>&1
    stamp=$(objdump -p build/nvngx_dlssnr.dll | grep -i "time/date" || true)
    t "proxy PE stamp is zero" sh -c 'printf "%s" "$1" | grep -q "1970"' sh "$stamp"
else
    echo "skip: proxy PE stamp (need mingw + objdump)"
fi

if [ "$fail" -ne 0 ]; then exit 1; fi
echo "PASS: reproducible build pins hold"
