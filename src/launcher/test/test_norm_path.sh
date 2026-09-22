#!/bin/sh
# SLOP T03 §1.4 golden exe-key vectors for the trampoline `norm_path`.
# Mirrors the Rust contract in tuxgt-core session/tests_1.rs and the Python
# mirror in src/protonfixes-hook/test_norm.py. Keep the three in sync so
# drift fails loudly. Self-contained: extracts the real `norm_path` from
# ../tuxgt-launcher (the script cannot be sourced — it execs) and runs the
# shared vectors. Run: `sh test_norm_path.sh` from this directory.
set -eu

dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
launcher=$dir/../tuxgt-launcher

eval "$(sed -n '/^norm_path() {/,/^}/p' "$launcher")"
command -v norm_path >/dev/null || { echo "FAIL: norm_path not extracted"; exit 1; }

fail=0
check() {
    got=$(norm_path "$1")
    if [ "$got" != "$2" ]; then
        echo "FAIL: input [$1] want [$2] got [$got]"
        fail=1
    fi
}

tab=$(printf '\t')
# Pre-existing Rust cases (kept verbatim).
check 'C:\Games\SkyrimSE.EXE ' 'c:/games/skyrimse.exe'
check '/PFX/Skyrim//' '/pfx/skyrim'
check '  /a / ' '/a'
check '' ''
# Backslash fold, lowercase, whitespace trim.
check 'a\b\c' 'a/b/c'
check 'ABC.EXE' 'abc.exe'
check "$tab /X/ " '/x'
# Trailing slashes trim; leading slashes stay.
check 'foo///' 'foo'
check '///a' '///a'
check '/' ''
check '   ' ''
# Combined: all folds at once.
check '  C:\GAMES\Foo.EXE//  ' 'c:/games/foo.exe'

if [ "$fail" -ne 0 ]; then exit 1; fi
echo "PASS: all norm_path golden vectors match"
