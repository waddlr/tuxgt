#!/bin/sh
# Re-entrancy pin for load_loaddll's LoadDLL walk: the loop body calls
# LoadLibraryW (Wine), which may use strtok internally and clobber a
# plain-strtok walk after the first entry (game saw Loaded=1/1 with two
# LoadDLL lines). Compiles the real load.c + deps with a strtok-dirtying
# fake loader; both entries must load. Run: `sh test_load_reentry.sh`
# from this directory.
set -eu

dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
src=$dir/..

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
mkdir -p "$tmp/depot" "$tmp/home"
printf a > "$tmp/depot/a.dll"
printf a > "$tmp/home/a.dll"
printf b > "$tmp/depot/b.dll"
printf b > "$tmp/home/b.dll"

cc -O2 -o "$tmp/harness" "$dir/test_load_reentry.c" "$src/load.c" \
    "$src/cfg.c" "$src/util.c" "$src/stage.c" "$src/pe.c" -I"$src" -lcrypto -ldl
"$tmp/harness" "$tmp/home" "$tmp/depot"
