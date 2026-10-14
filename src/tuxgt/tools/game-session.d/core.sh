#!/bin/sh
# game-session lane helpers: pid/lock discipline, lane env, capture stack.
# Sourced by src/tuxgt/tools/game-session after lane vars/args are set;
# functions only, no top-level side effects. Not a standalone command.

lane_binary() { [ -f "$lane/binary" ] && cat "$lane/binary" || echo "$binary"; }
lane_up() { [ -f "$lane/gamescope.pid" ] && kill -0 "$(cat "$lane/gamescope.pid")" 2>/dev/null; }
pid_of() { [ -f "$1" ] && cat "$1" || true; }
pid_alive() { p=$(pid_of "$1"); [ -n "$p" ] && kill -0 "$p" 2>/dev/null; }
child_env() { grep "^$1=" "$lane/child-env.txt" 2>/dev/null | head -1 | cut -d= -f2-; }

# Kill one lane pidfile's process, only when its cmdline+environ still
# matches a lane-unique token (never `pkill -f`, never trust a stale pid).
kill_pidfile() {
    p=$(pid_of "$1") || p=
    [ -z "$p" ] && return 0
    if kill -0 "$p" 2>/dev/null; then
        h=$(tr '\0' ' ' <"/proc/$p/cmdline" 2>/dev/null; tr '\0' ' ' <"/proc/$p/environ" 2>/dev/null)
        case $h in
            *"$2"*) kill "${3:-TERM}" "$p" 2>/dev/null || true ;;
            *) printf 'game-session: pid %s is not %s, not killing\n' "$p" "$2" >&2 ;;
        esac
        i=0
        while kill -0 "$p" 2>/dev/null && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
        if kill -0 "$p" 2>/dev/null; then
            h=$(tr '\0' ' ' <"/proc/$p/cmdline" 2>/dev/null; tr '\0' ' ' <"/proc/$p/environ" 2>/dev/null)
            case $h in
                *"$2"*) kill -KILL "$p" 2>/dev/null || true ;;
            esac
        fi
    fi
    rm -f "$1"
}

# Class locks: a flock held by a sleeper per lock file. Heavy takes its own
# lock plus every smoke slot, so the classes exclude each other both ways.
lock_try() {
    mkdir -p "$lane/locks"
    pf="$lane/locks/$1.pid"
    rm -f "$pf"
    # flock forks before running its command, so $! is not the holder: the
    # holder writes its own pid (kept across exec) or the lock leaks.
    nohup flock -n "$2" sh -c 'echo $$ >"$1"; exec sleep 2147483647' _ "$pf" \
        >/dev/null 2>&1 8>&- &
    i=0
    while [ $i -lt 20 ]; do
        [ -s "$pf" ] && kill -0 "$(cat "$pf")" 2>/dev/null && return 0
        sleep 0.1; i=$((i + 1))
    done
    return 1
}
lock_release() {
    for f in "$lane/locks"/*.pid; do [ -f "$f" ] && kill_pidfile "$f" "sleep 2147483647"; done
    return 0
}
release_temp() {
    for f in "$lane/locks"/t*.pid; do [ -f "$f" ] && kill_pidfile "$f" "sleep 2147483647"; done
    return 0
}
lock_acquire() {
    mkdir -p "$state_root"
    exec 8>"$state_root/.acquire.lock"
    flock 8
    ok=1
    case $class in
        heavy)
            lock_try heavy "$state_root/heavy.lock" || ok=0
            for s in 1 2 3; do lock_try "smoke$s" "$state_root/smoke$s.lock" || ok=0; done
            ;;
        smoke)
            ok=0
            for s in 1 2 3; do
                if lock_try "smoke$s" "$state_root/smoke$s.lock"; then ok=1; break; fi
            done
            ;;
        *) die "--class takes smoke or heavy" ;;
    esac
    flock -u 8
    exec 8>&-
    [ $ok -eq 1 ] || { lock_release; die "no $class slot free under $state_root (another lane is running; not queueing)"; }
}

# Lane env. `play` adds the Proton pins the spike proved: WINEPREFIX is the
# lane compatdata (umu ignores STEAM_COMPAT_DATA_PATH), Wine stays on the
# X11 driver, GE-Proton upscaler downloads stay off, audio is dummy/null.
export_env() {
    XDG_RUNTIME_DIR=$runtime
    PIPEWIRE_RUNTIME_DIR=$runtime
    DISPLAY=$(child_env DISPLAY)
    GAMESCOPE_WAYLAND_DISPLAY=$(child_env GAMESCOPE_WAYLAND_DISPLAY)
    TUXGT_DATA=$prefix
    TUXGT_CONFIG=$prefix/config
    export XDG_RUNTIME_DIR PIPEWIRE_RUNTIME_DIR DISPLAY GAMESCOPE_WAYLAND_DISPLAY TUXGT_DATA TUXGT_CONFIG
    unset WAYLAND_DISPLAY DBUS_SESSION_BUS_ADDRESS PULSE_SERVER
    [ "${1:-}" = play ] || unset WINEPREFIX STEAM_COMPAT_DATA_PATH
    if [ "${1:-}" = play ]; then
        WINEPREFIX=$compat
        PROTON_ENABLE_WAYLAND=0
        PROTON_USE_XALIA=0
        PROTON_DLSS_UPGRADE=0
        PROTON_FSR4_UPGRADE=0
        PROTON_XESS_UPGRADE=0
        PROTON_ENABLE_NGX_UPDATER=0
        PROTON_LOG=1
        PROTON_LOG_DIR=$lane/logs
        UMU_RUNTIME_UPDATE=0
        SDL_AUDIODRIVER=dummy
        ALSA_CONFIG_PATH=$lane/asoundrc
        export WINEPREFIX PROTON_ENABLE_WAYLAND PROTON_USE_XALIA PROTON_DLSS_UPGRADE \
            PROTON_FSR4_UPGRADE PROTON_XESS_UPGRADE PROTON_ENABLE_NGX_UPDATER \
            PROTON_LOG PROTON_LOG_DIR UMU_RUNTIME_UPDATE SDL_AUDIODRIVER ALSA_CONFIG_PATH
    fi
}

gpu_vk() {
    case $gpu in
        "") return 1 ;;
        *:*)
            printf '%s' "$gpu" | grep -Eq '^[0-9a-fA-F]{4}:[0-9a-fA-F]{4}$' || die "bad --gpu '$gpu' (use vendor:device or renderDNNN)"
            printf '%s' "$gpu" ;;
        *)
            d=/sys/class/drm/${gpu##*/}/device
            [ -f "$d/vendor" ] || die "bad --gpu '$gpu' (no such render node)"
            printf '%s:%s' "$(sed 's/^0x//' "$d/vendor")" "$(sed 's/^0x//' "$d/device")" ;;
    esac
}

wineserver_k() {
    [ -n "$proton" ] || return 1
    ws=$(dirname -- "$proton")/files/bin/wineserver
    [ -x "$ws" ] || return 1
    WINEPREFIX=$compat/pfx "$ws" -k >/dev/null 2>&1
}

stop_game() {
    p=$(pid_of "$lane/play.pid") || p=
    rm -f "$lane/play.pid"
    [ -n "$p" ] && kill -0 "$p" 2>/dev/null || return 0
    wineserver_k || true
    i=0
    while kill -0 "$p" 2>/dev/null && [ $i -lt 300 ]; do sleep 0.1; i=$((i + 1)); done
    kill -0 "$p" 2>/dev/null || return 0
    case $(tr '\0' ' ' <"/proc/$p/cmdline" 2>/dev/null || echo) in
        *"$lane/game"*|*umu*|*tuxgt-launcher*|*tuxgt*launch*|*proton*)
            kill -TERM "$p" 2>/dev/null || true
            i=0
            while kill -0 "$p" 2>/dev/null && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
            kill -0 "$p" 2>/dev/null && kill -KILL "$p" 2>/dev/null || true ;;
        *) printf 'game-session: play pid %s cmdline not recognised, not killing\n' "$p" >&2 ;;
    esac
}

pw_serial() {
    XDG_RUNTIME_DIR=$runtime PIPEWIRE_RUNTIME_DIR=$runtime pw-dump 2>/dev/null | python3 -c '
import json,sys
try:
    data=json.load(sys.stdin)
except Exception:
    sys.exit(1)
for o in data:
    p=o.get("info",{}).get("props",{}) or {}
    if p.get("node.name")=="gamescope":
        print(p.get("object.serial")); break
else:
    sys.exit(1)'
}

# Record: gst (PipeWire) -> FIFO -> ffmpeg (ffvhuff/mkv). Two pidfiles so
# stop can INT gst first and let ffmpeg finalize the mkv on FIFO EOF; a
# group INT mid-stream truncates the mux (proven in the idle smoke).
record_active() { pid_alive "$lane/record.pid"; }
record_start() {
    record_active && die "lane $id is already recording ($(cat "$lane/record.name" 2>/dev/null))"
    mkdir -p "$lane/video"
    serial=
    i=0
    while [ $i -lt 20 ]; do
        serial=$(pw_serial) && [ -n "$serial" ] && break
        sleep 0.5; i=$((i + 1))
    done
    [ -n "$serial" ] || die "no gamescope PipeWire node in lane $id (see $lane/logs/gamescope.log)"
    out=$lane/video/$1.mkv
    fifo=$lane/video/.$1.fifo
    rm -f "$fifo"
    mkfifo "$fifo"
    nohup ffmpeg -y -v error -f rawvideo -pix_fmt rgb24 -s "${W}x${H}" -framerate "$fps" \
        -i "$fifo" -an -c:v ffvhuff "$out" >>"$lane/logs/record.log" 2>&1 &
    echo $! >"$lane/record-ff.pid"
    XDG_RUNTIME_DIR=$runtime PIPEWIRE_RUNTIME_DIR=$runtime \
        nohup gst-launch-1.0 -q pipewiresrc target-object="$serial" ! videoconvert ! \
        "video/x-raw,format=RGB,width=$W,height=$H" ! fdsink fd=1 \
        >"$fifo" 2>>"$lane/logs/record.log" &
    echo $! >"$lane/record.pid"
    echo "$1" >"$lane/record.name"
    echo "$fifo" >"$lane/record.fifo"
    date +%s.%N >"$lane/video/$1.t0"
    printf 'record\tstart\t%s\n' "$out"
}
record_stop() {
    if ! record_active && ! pid_alive "$lane/record-ff.pid"; then
        [ "${1:-}" = quiet ] || die "lane $id is not recording"
        rm -f "$lane/record.pid" "$lane/record-ff.pid" "$lane/record.name" "$lane/record.fifo"
        return 0
    fi
    nm=$(cat "$lane/record.name" 2>/dev/null || echo record)
    out=$lane/video/$nm.mkv
    fifo=$(cat "$lane/record.fifo" 2>/dev/null || echo "$lane/video/.$nm.fifo")
    kill_pidfile "$lane/record.pid" gst-launch INT
    i=0
    while pid_alive "$lane/record-ff.pid" && [ $i -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
    kill_pidfile "$lane/record-ff.pid" ffmpeg INT
    rm -f "$lane/record.name" "$lane/record.fifo" "$fifo"
    printf 'record\tstop\t%s\t%s bytes\n' "$out" "$(wc -c <"$out" 2>/dev/null || echo 0)"
}

take_shot() {
    XDG_RUNTIME_DIR=$runtime GAMESCOPE_WAYLAND_DISPLAY=$(child_env GAMESCOPE_WAYLAND_DISPLAY) \
        gamescopectl screenshot "$1" >/dev/null 2>&1
    i=0
    while [ ! -s "$1" ] && [ $i -lt 10 ]; do sleep 0.1; i=$((i + 1)); done
    [ -s "$1" ]
}
nonblack() {
    ffmpeg -v error -i "$1" -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - 2>/dev/null |
        od -An -tu1 | awk '{for(i=1;i<=NF;i++) if($i>=16) f=1} END{exit !f}'
}

stop_stack() {
    record_stop quiet >/dev/null 2>&1 || true
    stop_game
    kill_pidfile "$lane/gamescope.pid" "$lane/keeper.sh"
    kill_pidfile "$lane/wireplumber.pid" "XDG_RUNTIME_DIR=$runtime"
    kill_pidfile "$lane/pipewire.pid" "PIPEWIRE_RUNTIME_DIR=$runtime"
    rm -f "$lane/child-env.txt"
}
start_fail() { stop_stack >/dev/null 2>&1 || true; lock_release; die "$*"; }

last_file() { ls -t "$1" 2>/dev/null | head -1; }

log_file() {
    case $1 in
        gamescope) echo "$lane/logs/gamescope.log" ;;
        game) last_file "$lane/logs/play-*.log" ;;
        proton) last_file "$lane/logs/steam-*.log" ;;
        launcher) find "$prefix/games" -path '*/runtime/tuxgt-launcher.log' 2>/dev/null | head -1 ;;
        reshade) find "$prefix/games" -path '*/runtime/ReShade.log' 2>/dev/null | head -1 ;;
    esac
}
