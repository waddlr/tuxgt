#!/bin/sh
# Prefix-fallback vectors for the trampoline `correlate_rel` (hook parity).
# Exact (prefix, exe) hits keep priority; an argv exe with no key of its
# own resolves when its prefix section names exactly one rel; contested
# prefixes and unknown prefixes miss. Self-contained like
# test_norm_path.sh: extracts the real functions from ../tuxgt-launcher
# (the script cannot be sourced — it execs) and runs lookups against a
# temp correlator. Run: `sh test_correlate.sh` from this directory.
set -eu

dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
launcher=$dir/../tuxgt-launcher

eval "$(sed -n '/^norm_path() {/,/^}/p;/^runtime_prefix() {/,/^}/p;/^runtime_exe() {/,/^}/p;/^correlate_rel() {/,/^}/p' "$launcher")"
command -v correlate_rel >/dev/null || { echo "FAIL: correlate_rel not extracted"; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
mkdir -p "$tmp/games"
cat > "$tmp/games/load-correlator.ini" <<'EOF'
# tuxgt load-correlator

[/pfx/mo2]
c:/games/stock game/skyrimse.exe=steam/mo2test
EOF

data=$tmp
export STEAM_COMPAT_DATA_PATH=/pfx/mo2
unset EXE || true

fail=0
check() {
    want=$1
    shift
    got=$(correlate_rel "$@" 2>/dev/null || true)
    if [ -z "$got" ]; then got=MISS; fi
    if [ "$got" != "$want" ]; then
        echo "FAIL: argv [$*] want [$want] got [$got]"
        fail=1
    fi
}

# Exact hit (mixed-case argv against a lowercased key).
check 'steam/mo2test' 'C:\Games\Stock Game\SkyrimSE.exe'
# Fallback hit: loader exe with no key, single-rel prefix.
check 'steam/mo2test' 'C:\Games\skse64_loader.exe'
check 'steam/mo2test' 'C:\Games\ModOrganizer.exe'

# Stray empty-valued key: ignored, single real rel still resolves.
printf '%s\n' 'c:/games/stray/stray.exe=' >> "$tmp/games/load-correlator.ini"
check 'steam/mo2test' 'C:\Games\skse64_loader.exe'

# Contested prefix: exact still wins, unknown exe misses.
printf '%s\n' 'c:/games/tool/tool.exe=heroic_gog/abc' >> "$tmp/games/load-correlator.ini"
check 'heroic_gog/abc' 'c:\games\tool\tool.exe'
check 'MISS' 'C:\Games\skse64_loader.exe'

# Unknown prefix misses.
export STEAM_COMPAT_DATA_PATH=/pfx/unknown
check 'MISS' 'C:\Games\skse64_loader.exe'

if [ "$fail" -ne 0 ]; then exit 1; fi
echo "PASS: all correlate_rel fallback vectors match"
