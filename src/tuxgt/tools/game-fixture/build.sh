#!/bin/sh
# Build the D3D11 e2e fixture exe on demand (tools do not ship; no PE
# is committed). Usage: build.sh [OUTDIR]   (default: ./build)
# Toolchain override: MINGW_CXX=/path/to/mingw-g++
set -eu

die() { echo "game-fixture: $*" >&2; exit 1; }

src_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
out=${1:-"$src_dir/build"}
cxx=${MINGW_CXX:-x86_64-w64-mingw32-g++}

command -v "$cxx" >/dev/null 2>&1 ||
    die "mingw-w64 g++ not found ($cxx); install mingw-w64 or set MINGW_CXX"
[ -f "$src_dir/fixture.cpp" ] || die "fixture.cpp missing beside build.sh"

mkdir -p "$out"
exe="$out/tuxgt-fixture-d3d11.exe"
"$cxx" -O2 -march=x86-64 -mtune=generic \
    -Wl,--no-insert-timestamp -mwindows -static-libgcc -static-libstdc++ \
    -o "$exe" "$src_dir/fixture.cpp" -ld3d11 -ldxgi -luser32 -lgdi32
echo "built: $exe"
sha256sum "$exe"
