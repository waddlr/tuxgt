#!/usr/bin/env bash
# In-game ReShade proof (MAP e2e.game-proof, Preload/D3D11 cell): fresh
# game-session lane -> games add the D3D11 fixture -> doctor pins Proton ->
# Hook on -> vanilla play -> install+enable official ReShade via the real
# CLI -> channel-swap preset in the per-game runtime dir -> play -> oracle
# set -> drive-script overlay toggle (Home, Skip Tutorial click, Home) ->
# negative controls (disabled, then uninstalled relaunch) -> HTML report.
#
# Local/nightly only: needs a real GPU, GE-Proton (or GG_PROTON=<proton
# script>), gamescope/pipewire/gstreamer, mingw-w64, cargo, and a packaged
# `tuxgt install` once (protonfixes localfixes shim). Evidence bundles land
# in the lane under bundles/<runstamp>/ and are never committed.
#
# Run from the repo root: bash tests/test_game_reshade.sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
GS=$root/src/tuxgt/tools/game-session
BIN=$root/src/tuxgt/target/debug/tuxgt
state_root=${TUXGT_GG_ROOT:-/tmp/tuxgt-gg}
id=${GG_ID:-reshade-e2e}
lane=$state_root/$id
runstamp=$(date +%Y%m%d-%H%M%S)
bundle=$lane/bundles/$runstamp
PROTON=${GG_PROTON:-}
if [ -z "$PROTON" ]; then
    for d in "$HOME"/.steam/root/compatibilitytools.d/GE-Proton*/proton \
             "$HOME"/.steam/root/compatibilitytools.d/*/proton; do
        [ -f "$d" ] && { PROTON=$d; break; }
    done
fi

for c in gamescope gamescopectl pipewire wireplumber pw-dump gst-launch-1.0 \
         ffmpeg ffprobe xdotool flock python3 cargo x86_64-w64-mingw32-g++ sha256sum; do
    command -v "$c" >/dev/null 2>&1 || { echo "FAIL: need '$c' installed" >&2; exit 1; }
done
[ -n "$PROTON" ] && [ -f "$PROTON" ] || { echo "FAIL: no Proton found (set GG_PROTON)" >&2; exit 1; }
[ -f "$HOME/.config/protonfixes/localfixes/default.py" ] || {
    echo "FAIL: ~/.config/protonfixes/localfixes/default.py missing (run a packaged 'tuxgt install' once)" >&2; exit 1; }

mkdir -p "$bundle"
fail=0
say() { printf '%s\n' "$*" | tee -a "$bundle/result.txt"; }
ok() { say "ok: $*"; }
bad() { say "FAIL: $*"; fail=1; }
ggcli() {
    printf '+ tuxgt %s\n' "$*" >>"$bundle/commands.txt"
    "$GS" cli --id "$id" -- "$@" 2>>"$bundle/cli.log"
}
cleanup() { "$GS" stop --id "$id" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM

load_count() {
    local n=0
    n=$(grep -c 'load from LoadLibrary' "$runtime/tuxgt-launcher.log" 2>/dev/null) || n=${n:-0}
    printf '%s' "$n"
}
mean_at() { # png x y w h -> "R G B"
    ffmpeg -v error -i "$1" -vf "crop=$4:$5:$2:$3,scale=1:1" -f rawvideo -pix_fmt rgb24 - 2>/dev/null |
        od -An -tu1 | tr -s ' ' | sed 's/^ //;s/ $//'
}
ge90() { [ "$2" -gt 0 ] && [ $(( $1 * 100 )) -ge $(( 90 * $2 )) ]; }

# Classify extracted frames by the BL quadrant sample (128,252 64x36):
# B=fixture blue (32,32,200), R=swapped red (200,32,32), K=black, O=other.
# Post-startup window = last 60% after the first non-black frame; banner
# window = first 60% after it (top strip 8,6 200x24 darkens under banner).
A_TOTAL=0 A_POST=0 A_BLUE=0 A_RED=0 A_BANNER=0 A_AFTER_BLUE=0 A_BLACK=0
analyze() {
    local dir=$1 frames=() cls=() f bl r g b mx first=-1 idx=0 post=0 bend=0 seenred=0 top tr tg tb
    A_TOTAL=0 A_POST=0 A_BLUE=0 A_RED=0 A_BANNER=0 A_AFTER_BLUE=0 A_BLACK=0
    for f in "$dir"/*.png; do [ -f "$f" ] && frames+=("$f"); done
    A_TOTAL=${#frames[@]}
    [ "$A_TOTAL" -gt 0 ] || return 0
    for f in "${frames[@]}"; do
        bl=$(mean_at "$f" 128 252 64 36); set -- $bl
        r=${1:-0}; g=${2:-0}; b=${3:-0}
        mx=$r; [ "$g" -gt "$mx" ] && mx=$g; [ "$b" -gt "$mx" ] && mx=$b
        if [ "$mx" -lt 16 ]; then cls+=(K)
        elif [ "$b" -gt 140 ] && [ "$r" -lt 90 ]; then cls+=(B)
        elif [ "$r" -gt 140 ] && [ "$b" -lt 90 ]; then cls+=(R)
        else cls+=(O); fi
    done
    for ((idx=0; idx<A_TOTAL; idx++)); do
        [ "${cls[$idx]}" != K ] && { first=$idx; break; }
    done
    [ "$first" -ge 0 ] || return 0
    post=$(( first + (A_TOTAL - first) * 2 / 5 ))
    bend=$(( first + (A_TOTAL - first) * 3 / 5 ))
    for ((idx=first; idx<A_TOTAL; idx++)); do
        case ${cls[$idx]} in
            K) A_BLACK=$((A_BLACK + 1)) ;;
            R) seenred=1; [ "$idx" -ge "$post" ] && A_RED=$((A_RED + 1)) ;;
            B)
                [ "$seenred" -eq 1 ] && A_AFTER_BLUE=$((A_AFTER_BLUE + 1))
                [ "$idx" -ge "$post" ] && A_BLUE=$((A_BLUE + 1)) ;;
        esac
        [ "$idx" -ge "$post" ] && A_POST=$((A_POST + 1))
        if [ "$idx" -le "$bend" ] && { [ "${cls[$idx]}" = B ] || [ "${cls[$idx]}" = R ]; }; then
            top=$(mean_at "${frames[$idx]}" 8 6 200 24); set -- $top
            tr=${1:-0}; tg=${2:-0}; tb=${3:-0}
            [ $((tr + tg + tb)) -lt 220 ] && A_BANNER=1
        fi
    done
}

extract() { # video outdir
    mkdir -p "$2"
    [ -f "$1" ] || return 0
    ffmpeg -y -v error -i "$1" -vf fps=5 "$2/%04d.png" >/dev/null 2>&1 || true
}
snapshot_logs() { # label
    local d=$bundle/logs-$1 newest
    mkdir -p "$d"
    cp "$runtime/ReShade.log" "$runtime/tuxgt-launcher.log" "$d/" 2>/dev/null || true
    cp "$lane/logs/play-$2.log" "$d/" 2>/dev/null || true
    cp "$lane/game/fixture.log" "$d/" 2>/dev/null || true
    newest=$(ls -t "$lane"/logs/steam-*.log 2>/dev/null | head -1 || true)
    [ -n "$newest" ] && cp "$newest" "$d/proton.log" || true
}
run_play() { # label
    local pname=$1-$runstamp
    if "$GS" play --id "$id" --seconds 10 --serial --name "$pname" "$gid" \
            >>"$bundle/play-cli.log" 2>&1; then
        ok "play $1 completed"
    else
        bad "play $1 exited nonzero (see bundle/play-cli.log)"
    fi
    extract "$lane/video/$pname.mkv" "$bundle/frames-$1"
    analyze "$bundle/frames-$1"
    snapshot_logs "$1" "$pname"
}
expect_vanilla() { # label
    [ "$A_TOTAL" -ge 20 ] && ok "$1 liveness (frames=$A_TOTAL)" || bad "$1 liveness (frames=$A_TOTAL)"
    if ge90 "$A_BLUE" "$A_POST"; then ok "$1 BL stays fixture blue (blue=$A_BLUE post=$A_POST)"
    else bad "$1 BL not blue (blue=$A_BLUE red=$A_RED post=$A_POST)"; fi
    if [ "$A_BANNER" -eq 0 ]; then ok "$1 has no ReShade banner"
    else bad "$1 unexpectedly shows a banner-like top strip"; fi
}

say "game reshade e2e run $runstamp lane=$id proton=$PROTON"
(cd "$root/src/tuxgt" && cargo build -p tuxgt) >"$bundle/cargo.log" 2>&1 ||
    { echo "FAIL: cargo build -p tuxgt (see $bundle/cargo.log)" >&2; exit 1; }
[ -x "$BIN" ] || { echo "FAIL: binary missing after build: $BIN" >&2; exit 1; }
"$GS" stop --id "$id" >/dev/null 2>&1 || true
"$GS" start --id "$id" --class smoke --seed --fresh --proton "$PROTON" \
    >>"$bundle/lane.log" 2>&1 || { echo "FAIL: lane start (see $bundle/lane.log)" >&2; exit 1; }

# Fixture build comes after start: --fresh wipes the lane game dir.
"$root/src/tuxgt/tools/game-fixture/build.sh" "$lane/game" >"$bundle/fixture-build.log" 2>&1 ||
    { echo "FAIL: fixture build (see $bundle/fixture-build.log)" >&2; exit 1; }
exe=$lane/game/tuxgt-fixture-d3d11.exe
sha256sum "$exe" >>"$bundle/fixtures.sha256"

ggcli games add "$exe" >/dev/null || { echo "FAIL: games add (see $bundle/cli.log)" >&2; exit 1; }
gid=$(ggcli games list | grep -o 'manual:standalone:[A-Za-z0-9_-]*' | head -1)
[ -n "$gid" ] || { echo "FAIL: manual game id not found after games add" >&2; exit 1; }
say "game id: $gid"
gdir=$lane/prefix/games/manual_standalone/${gid##*:}
runtime=$gdir/runtime
ggcli doctor "$gid" --set platform=proton --set "proton=$PROTON" \
    --set "prefix=$lane/compatdata" >/dev/null || { echo "FAIL: doctor --set (see $bundle/cli.log)" >&2; exit 1; }
aout=$(ggcli games adapter "$gid") || { echo "FAIL: games adapter (see $bundle/cli.log)" >&2; exit 1; }
case $aout in *preload*) ok "adapter is preload" ;; *) bad "adapter not preload: $aout" ;; esac

c0=$(load_count)
run_play vanilla
expect_vanilla vanilla
[ "$(load_count)" -eq "$c0" ] && ok "vanilla: no launcher DLL loads" || bad "vanilla: unexpected DLL load"
grep -q 'first-frame' "$bundle/logs-vanilla/fixture.log" 2>/dev/null &&
    ok "vanilla: fixture presented (first-frame)" || bad "vanilla: fixture first-frame not logged"

ggcli instance install "$gid" reshade --adapter preload --yes >/dev/null || { echo "FAIL: instance install (see $bundle/cli.log)" >&2; exit 1; }
ggcli instance enable "$gid" reshade --yes >/dev/null || { echo "FAIL: instance enable (see $bundle/cli.log)" >&2; exit 1; }
# Handle arms the protonfixes Hook session; its sync sees the enabled
# preload instance only when handle flips after enable (scratch-proven).
ggcli games handle "$gid" --on >/dev/null || { echo "FAIL: games handle --on (see $bundle/cli.log)" >&2; exit 1; }
sout=$(ggcli instance status "$gid") || { echo "FAIL: instance status (see $bundle/cli.log)" >&2; exit 1; }
case $sout in *ReShade64.dll*in-sync*) ok "reshade installed in-sync" ;; *) bad "instance status: $sout" ;; esac
# The session sync arms inject only once an enabled preload instance exists.
if grep -q '^inject=1' "$gdir/tux-protonfixes.conf" 2>/dev/null &&
   grep -q '^TUXGT_LAUNCHER_SO=' "$gdir/tux-protonfixes.conf" 2>/dev/null; then
    ok "session conf armed (inject=1 + launcher .so)"
else
    bad "session conf not armed (see $gdir/tux-protonfixes.conf)"
fi

mkdir -p "$runtime" "$bundle/preset"
cat >"$runtime/ChannelSwap.fx" <<'FX'
// Test effect for game-e2e-visual: swap red and blue channels.
// BL blue (32,32,200) -> (200,32,32); TL red -> blue.
texture BackBufferTex : COLOR;
sampler BackBuffer { Texture = BackBufferTex; AddressU = CLAMP; AddressV = CLAMP; };
void VS_PostProcess(in uint id : SV_VertexID, out float4 pos : SV_Position, out float2 uv : TEXCOORD)
{
    uv = float2((id << 1) & 2, id & 2);
    pos = float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
}
float4 PS_Swap(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target
{
    float4 c = tex2D(BackBuffer, uv);
    return float4(c.b, c.g, c.r, c.a);
}
technique ChannelSwap { pass { VertexShader = VS_PostProcess; PixelShader = PS_Swap; } }
FX
cat >"$runtime/ReShadePreset.ini" <<'PRE'
PreprocessorDefinitions=
Techniques=ChannelSwap@ChannelSwap.fx
TechniqueSorting=ChannelSwap@ChannelSwap.fx
PRE
cat >"$runtime/ReShade.ini" <<'INI'
[GENERAL]
EffectSearchPaths=.\**
TextureSearchPaths=.\**
PresetPath=.\ReShadePreset.ini
PresetTransitionDuration=0
TutorialProgress=4
INI
cp "$runtime/ChannelSwap.fx" "$runtime/ReShadePreset.ini" "$runtime/ReShade.ini" "$bundle/preset/"

c1=$(load_count)
run_play reshade
[ "$A_TOTAL" -ge 20 ] && ok "reshade liveness (frames=$A_TOTAL)" || bad "reshade liveness (frames=$A_TOTAL)"
if ge90 "$A_RED" "$A_POST"; then ok "reshade A/B: BL swapped to red (red=$A_RED post=$A_POST)"
else bad "reshade A/B: BL not swapped (red=$A_RED blue=$A_BLUE post=$A_POST)"; fi
[ "$A_BANNER" -eq 1 ] && ok "reshade banner seen in startup window" || bad "reshade banner never seen"
[ "$A_AFTER_BLUE" -eq 0 ] && ok "reshade persistence: no blue after first swapped frame" ||
    bad "reshade persistence: $A_AFTER_BLUE blue frames after swap"
[ "$A_BLACK" -eq 0 ] && ok "reshade flicker: no black frames after first frame" ||
    bad "reshade flicker: $A_BLACK black frames"
[ "$(load_count)" -gt "$c1" ] && ok "launcher log: ReShade64.dll loaded" ||
    bad "launcher log: no new LoadLibrary load line"
rlog=$bundle/logs-reshade/ReShade.log
grep -aq "Initializing crosire's ReShade" "$rlog" 2>/dev/null &&
    ok "ReShade.log: runtime initialized" || bad "ReShade.log: init line missing"
grep -aq "Successfully compiled '.*ChannelSwap\\.fx'" "$rlog" 2>/dev/null &&
    ok "ReShade.log: ChannelSwap.fx compiled" || bad "ReShade.log: ChannelSwap compile line missing"
if [ -f "$bundle/logs-reshade/proton.log" ]; then
    grep -aiq 'fatal' "$bundle/logs-reshade/proton.log" &&
        bad "proton log contains 'fatal'" || ok "proton log: no fatal error"
else
    bad "proton log missing from bundle"
fi

# Overlay scenario: caller-scripted interaction via a drive file (the
# tool holds keys so ReShade's per-frame key poll cannot miss them).
# Panel region (20,200 100x60) is swapped-red fixture with no overlay,
# dark ReShade UI while the overlay is up.
cat >"$bundle/overlay.drive" <<'DRIVE'
wait 4
shot pre-overlay
key Home
wait 2
shot overlay-open
click 190 335
wait 2
shot overlay-skip
key Home
wait 2
shot overlay-closed
DRIVE
oname=overlay-$runstamp
if "$GS" play --id "$id" --seconds 15 --serial --name "$oname" \
        --drive "$bundle/overlay.drive" "$gid" >>"$bundle/play-cli.log" 2>&1; then
    ok "play overlay completed"
else
    bad "play overlay exited nonzero (see bundle/play-cli.log)"
fi
cp "$lane/logs/drive-$oname.log" "$bundle/" 2>/dev/null || true
mkdir -p "$bundle/frames-overlay"
for n in pre-overlay overlay-open overlay-skip overlay-closed; do
    f=$(ls -t "$lane/frames"/*-"$n".png 2>/dev/null | head -1 || true)
    [ -n "$f" ] && cp "$f" "$bundle/frames-overlay/$n.png" || true
done
bl_class() { # png -> R|B|? by the analyze BL sample rule
    local m r b
    m=$(mean_at "$1" 128 252 64 36); set -- $m; r=${1:-0}; b=${3:-0}
    if [ "$r" -gt 140 ] && [ "$b" -lt 90 ]; then echo R
    elif [ "$b" -gt 140 ] && [ "$r" -lt 90 ]; then echo B
    else echo "?($r,$b)"; fi
}
panel_sum() { # png -> R+G+B mean over the overlay panel region
    local m
    m=$(mean_at "$1" 20 200 100 60); set -- $m
    echo $(( ${1:-0} + ${2:-0} + ${3:-0} ))
}
opre=$bundle/frames-overlay/pre-overlay.png
oopen=$bundle/frames-overlay/overlay-open.png
oskip=$bundle/frames-overlay/overlay-skip.png
oshut=$bundle/frames-overlay/overlay-closed.png
for f in "$opre" "$oopen" "$oskip" "$oshut"; do
    [ -f "$f" ] || bad "overlay: drive shot missing: ${f##*/}"
done
if [ -f "$opre" ]; then
    [ "$(bl_class "$opre")" = R ] && ok "overlay: effect active before Home (BL red)" ||
        bad "overlay: BL not swapped before Home ($(bl_class "$opre"))"
    [ "$(panel_sum "$opre")" -gt 220 ] && ok "overlay: panel region is fixture before Home" ||
        bad "overlay: panel region dark before Home (sum=$(panel_sum "$opre"))"
fi
if [ -f "$oopen" ]; then
    [ "$(panel_sum "$oopen")" -lt 220 ] && ok "overlay: Home opened the ReShade overlay (panel dark)" ||
        bad "overlay: overlay not visible after Home (sum=$(panel_sum "$oopen"))"
fi
if [ -f "$oskip" ]; then
    # Wizard buttons are blue (strip B~125+ at 20,325 200x18); after a
    # real Skip the main overlay replaces them (B~75). A mere hover
    # leaves the buttons lit, so a frame-diff alone would false-pass.
    sm=$(mean_at "$oskip" 20 325 200 18); set -- $sm; sb=${3:-0}
    if [ "$(panel_sum "$oskip")" -lt 220 ] && [ "$sb" -lt 100 ]; then
        ok "overlay: Skip Tutorial click left the wizard (main overlay up)"
    else
        bad "overlay: wizard still up after click (stripB=$sb sum=$(panel_sum "$oskip"))"
    fi
fi
if [ -f "$oshut" ]; then
    if [ "$(bl_class "$oshut")" = R ] && [ "$(panel_sum "$oshut")" -gt 220 ]; then
        ok "overlay: second Home closed the overlay, effect still on"
    else
        bad "overlay: not back to swapped game after close (BL=$(bl_class "$oshut") sum=$(panel_sum "$oshut"))"
    fi
fi

ggcli instance disable "$gid" reshade >/dev/null || { echo "FAIL: instance disable (see $bundle/cli.log)" >&2; exit 1; }
c2=$(load_count)
run_play disabled
expect_vanilla disabled
[ "$(load_count)" -eq "$c2" ] && ok "disabled: no new DLL loads" || bad "disabled: DLL loaded anyway"

ggcli instance uninstall "$gid" reshade --yes >/dev/null || { echo "FAIL: instance uninstall (see $bundle/cli.log)" >&2; exit 1; }
c3=$(load_count)
run_play uninstalled
expect_vanilla uninstalled
[ "$(load_count)" -eq "$c3" ] && ok "uninstalled: no new DLL loads" || bad "uninstalled: DLL loaded anyway"

{
    echo "run: $runstamp"
    gamescope --version 2>&1 | head -1 | sed 's/\x1b\[[0-9;]*m//g'
    echo "proton: $PROTON"
    echo "proton version file: $(cat "$(dirname "$PROTON")/version" 2>/dev/null || echo unknown)"
    grep -ao "ReShade version '[^']*'" "$rlog" 2>/dev/null | head -1 || true
    grep -a 'selecting physical device' "$lane/logs/gamescope.log" 2>/dev/null |
        tail -1 | sed 's/\x1b\[[0-9;]*m//g' || true
    echo "binary: $BIN"
} >"$bundle/versions.txt" 2>/dev/null || true
cp "$lane/logs/gamescope.log" "$bundle/" 2>/dev/null || true
cp "$gdir/tux-protonfixes.conf" "$gdir/tuxgt-launcher.ini" "$bundle/" 2>/dev/null || true
for p in vanilla reshade overlay disabled uninstalled; do
    cp "$lane/video/$p-$runstamp.mkv" "$lane/video/$p-$runstamp.times" "$bundle/" 2>/dev/null || true
done
# Human proof report: pin scenario frames + captions, then render.
mid_frame() { # scenario -> bundle-relative middle frame, or empty
    local list n f
    list=$(ls "$bundle/frames-$1"/*.png 2>/dev/null || true)
    [ -n "$list" ] || return 0
    n=$(printf '%s\n' "$list" | wc -l)
    f=$(printf '%s\n' "$list" | sed -n "$((n / 2 + 1))p")
    printf 'frames-%s/%s' "$1" "${f##*/}"
}
tab=$(printf '\t')
: >"$bundle/report.tsv"
row() { [ -n "$2" ] && [ -f "$bundle/$2" ] && printf '%s%s%s%s%s%s%s\n' "$1" "$tab" "$2" "$tab" "$3" "$tab" "$4" >>"$bundle/report.tsv" || true; }
row vanilla "$(mid_frame vanilla)" "No mod: fixture colors unchanged (bottom-left blue), no banner" "vanilla-$runstamp.mkv"
row reshade "$(mid_frame reshade)" "ReShade via TuxGT Preload + ChannelSwap: banner up, red/blue swapped (bottom-left red)" "reshade-$runstamp.mkv"
row overlay "frames-overlay/overlay-open.png" "Drive script pressed Home: ReShade overlay open in-game (Skip click + close in frames-overlay/)" "overlay-$runstamp.mkv"
row disabled "$(mid_frame disabled)" "Mod disabled: original colors back, no banner" "disabled-$runstamp.mkv"
row uninstalled "$(mid_frame uninstalled)" "Mod uninstalled: original colors back, no banner" "uninstalled-$runstamp.mkv"
say "bundle: $bundle"
if [ "$fail" -ne 0 ]; then
    say "FAIL: game reshade e2e"
else
    say "PASS: game reshade e2e (bundle: $bundle)"
fi
"$GS" report "$bundle" >>"$bundle/play-cli.log" 2>&1 ||
    { echo "FAIL: report generation (see $bundle/play-cli.log)" >&2; exit 1; }
say "report: $bundle/report/index.html"
exit "$fail"
