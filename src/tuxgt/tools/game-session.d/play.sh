#!/bin/sh
# game-session play: the timed, recorded launch of one GAMEID.
# Sourced by src/tuxgt/tools/game-session after lane vars/args are set;
# functions only, no top-level side effects. Not a standalone command.

cmd_play() {
    lane_up || die "lane $id is not running (start it first)"
    [ -f "$lane/class" ] || die "lane $id has no class — start with --class smoke|heavy first"
    set -- $args
    [ $# -ge 1 ] && [ -n "${1:-}" ] || die "play needs GAMEID (from: game-session cli -- games add <EXE>)"
    gid=$1
    pid_alive "$lane/play.pid" && die "lane $id already has a play running (pid $(cat "$lane/play.pid"))"
    bin=$(lane_binary)
    [ -x "$bin" ] || die "binary not found: $bin (build: cd src/tuxgt && cargo build -p tuxgt)"
    [ -z "$drive" ] || [ -f "$drive" ] || die "drive file not found: $drive"
    [ -z "$drive" ] || command -v xdotool >/dev/null 2>&1 || die "xdotool not installed (play --drive needs it)"
    [ -n "$name" ] || name=play$(($(ls "$lane/video"/*.times 2>/dev/null | wc -l) + 1))
    trap release_temp EXIT INT TERM
    if [ $serial -eq 1 ] && [ "$class" != heavy ]; then
        lock_try theavy "$state_root/heavy.lock" || die "--serial: heavy lock busy"
        for s in 1 2 3; do
            [ -f "$lane/locks/smoke$s.pid" ] && continue
            lock_try "tsmoke$s" "$state_root/smoke$s.lock" || die "--serial: smoke slot $s held by another lane"
        done
    fi
    mkdir -p "$lane/logs" "$lane/frames" "$lane/video"
    times=$lane/video/$name.times
    : >"$times"
    mine=0
    record_active || { record_start "$name"; mine=1; }
    printf 'spawn %s\n' "$(date +%s.%N)" >>"$times"
    export_env play
    echo "$gid" >"$lane/gameid"
    nohup "$bin" launch "$gid" >>"$lane/logs/play-$name.log" 2>&1 &
    echo $! >"$lane/play.pid"
    probe=$lane/frames/.probe-$name.png
    ff=0; deadline=$(($(date +%s) + 120))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        pid_alive "$lane/play.pid" || break
        rm -f "$probe"
        if take_shot "$probe" && nonblack "$probe"; then ff=1; break; fi
        sleep 0.5
    done
    if [ $ff -eq 0 ]; then
        tail -10 "$lane/logs/play-$name.log" >&2 || true
        stop_game; [ $mine -eq 1 ] && record_stop quiet >/dev/null || true
        die "play $name: no non-black frame within 120s (see $lane/video/$name.mkv and $lane/logs/)"
    fi
    printf 'firstframe %s\n' "$(date +%s.%N)" >>"$times"
    # --drive runs concurrently from first frame; play then lasts until
    # both --seconds have elapsed and the drive script has finished.
    drive_pid=
    if [ -n "$drive" ]; then
        printf 'drivestart %s\n' "$(date +%s.%N)" >>"$times"
        : >"$lane/logs/drive-$name.log"
        ( run_drive "$drive" >>"$lane/logs/drive-$name.log" 2>&1 ) &
        drive_pid=$!
        trap 'kill "$drive_pid" 2>/dev/null || true; release_temp' EXIT INT TERM
    fi
    t=$seconds; died=0
    while [ "$t" -gt 0 ]; do
        pid_alive "$lane/play.pid" || { died=1; break; }
        sleep 1; t=$((t - 1))
    done
    drive_rc=0
    if [ -n "$drive_pid" ]; then
        [ $died -eq 0 ] || kill "$drive_pid" 2>/dev/null || true
        wait "$drive_pid" || drive_rc=$?
        printf 'driveend %s\n' "$(date +%s.%N)" >>"$times"
    fi
    printf 'stop %s\n' "$(date +%s.%N)" >>"$times"
    stop_game
    [ $mine -eq 1 ] && record_stop >/dev/null
    [ $died -eq 0 ] || die "play $name: game exited before --seconds $seconds elapsed (see $lane/logs/play-$name.log)"
    [ "$drive_rc" -eq 0 ] || die "play $name: drive script failed (rc=$drive_rc, see $lane/logs/drive-$name.log)"
    [ $mine -eq 0 ] || [ -s "$lane/video/$name.mkv" ] || die "play $name: recorded video is empty (see $lane/logs/record.log)"
    trap - EXIT INT TERM
    release_temp
    printf 'play\t%s\tgame=%s\tvideo=%s\ttimes=%s\tlog=%s\n' \
        "$name" "$gid" "$lane/video/$name.mkv" "$times" "$lane/logs/play-$name.log"
}
