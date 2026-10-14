#!/bin/sh
# game-session input: xdotool primitives + the drive-file runner.
# Sourced by src/tuxgt/tools/game-session after lane vars/args are set;
# functions only, no top-level side effects. Not a standalone command.
#
# Keys go to the lane's single visible game window via a window-targeted
# xdotool send (the path the overlay spike proved toggles ReShade's
# overlay); with no visible window they fall back to the focused window.
# Mouse uses plain XTest: the gamescope window is fullscreen at 0,0, so
# lane pixel coordinates are root coordinates.
#
# Keys are held, not tapped: ReShade polls key state per presented
# frame, so a millisecond tap can fall entirely between two polls and
# never register (spike: taps toggled the overlay only sporadically,
# a 0.6s hold toggled it). lane_key holds each key ~0.35s.

game_wid() {
    xdotool search --onlyvisible --name . 2>/dev/null | head -1
}

lane_key() {
    export_env
    [ -n "$DISPLAY" ] || die "lane $id has no DISPLAY (start it first)"
    wid=$(game_wid)
    for k in "$@"; do
        if [ -n "$wid" ]; then
            xdotool keydown --window "$wid" "$k" || die "xdotool keydown $k failed"
            sleep 0.35
            xdotool keyup --window "$wid" "$k" || die "xdotool keyup $k failed"
        else
            xdotool keydown "$k" || die "xdotool keydown $k failed"
            sleep 0.35
            xdotool keyup "$k" || die "xdotool keyup $k failed"
        fi
    done
}

lane_type() {
    export_env
    [ -n "$DISPLAY" ] || die "lane $id has no DISPLAY (start it first)"
    wid=$(game_wid)
    if [ -n "$wid" ]; then
        xdotool type --delay 60 --window "$wid" -- "$1" || die "xdotool type failed"
    else
        xdotool type --delay 60 -- "$1" || die "xdotool type failed"
    fi
}

lane_xy() {
    case $1$2 in *[!0-9]*) die "coordinates take pixel numbers, got '$1 $2'" ;; esac
    [ "$1" -lt "$W" ] && [ "$2" -lt "$H" ] || die "point $1,$2 is outside the lane size ${W}x${H}"
}

lane_move() {
    export_env
    [ -n "$DISPLAY" ] || die "lane $id has no DISPLAY (start it first)"
    lane_xy "$1" "$2"
    xdotool mousemove --sync "$1" "$2" || die "xdotool mousemove failed"
}

lane_click() {
    # Held like lane_key: an instant down/up can fall between the
    # overlay's per-frame input polls and only the hover registers.
    lane_move "$1" "$2"
    xdotool mousedown "${3:-1}" || die "xdotool mousedown failed"
    sleep 0.25
    xdotool mouseup "${3:-1}" || die "xdotool mouseup failed"
}

cmd_key() {
    lane_up || die "lane $id is not running"
    [ -n "$args" ] || die "key needs a keysym: game-session key Home (combos: ctrl+c)"
    # shellcheck disable=SC2086
    lane_key $args
    printf 'key\t%s\n' "$args"
}

cmd_type() {
    lane_up || die "lane $id is not running"
    [ -n "$args" ] || die "type needs text: game-session type ChannelSwap"
    lane_type "$args"
    printf 'type\t%s\n' "$args"
}

cmd_move() {
    lane_up || die "lane $id is not running"
    set -- $args
    [ $# -eq 2 ] || die "move takes X Y (lane pixels)"
    lane_move "$1" "$2"
    printf 'move\t%s\t%s\n' "$1" "$2"
}

cmd_click() {
    lane_up || die "lane $id is not running"
    set -- $args
    [ $# -ge 2 ] && [ $# -le 3 ] || die "click takes X Y [BUTTON] (lane pixels)"
    lane_click "$1" "$2" "${3:-1}"
    printf 'click\t%s\t%s\tbutton=%s\n' "$1" "$2" "${3:-1}"
}

# Drive file: one action per line (wait S | key K... | type TEXT |
# click X Y [B] | move X Y | shot NAME); blank lines and lines starting
# with # are skipped, anything else dies. Each executed line is echoed
# with a timestamp to stdout (play redirects it to logs/drive-<run>.log).
run_drive() {
    [ -f "$1" ] || die "drive file not found: $1"
    while IFS= read -r line || [ -n "$line" ]; do
        case $line in ''|'#'*) continue ;; esac
        verb=${line%% *}
        rest=${line#"$verb"}
        rest=${rest# }
        printf 'drive\t%s\t%s\n' "$(date +%s.%N)" "$line"
        case $verb in
            wait)
                case $rest in *[!0-9.]*|'') die "drive: wait takes seconds, got '$rest'" ;; esac
                sleep "$rest" ;;
            key)
                [ -n "$rest" ] || die "drive: key needs a keysym"
                # shellcheck disable=SC2086
                lane_key $rest ;;
            type)
                [ -n "$rest" ] || die "drive: type needs text"
                lane_type "$rest" ;;
            click|move)
                # shellcheck disable=SC2086
                set -- $rest
                if [ "$verb" = click ]; then
                    [ $# -ge 2 ] && [ $# -le 3 ] || die "drive: click takes X Y [BUTTON]"
                    lane_click "$1" "$2" "${3:-1}"
                else
                    [ $# -eq 2 ] || die "drive: move takes X Y"
                    lane_move "$1" "$2"
                fi ;;
            shot)
                [ -n "$rest" ] || die "drive: shot needs a name"
                case $rest in *[!A-Za-z0-9_-]*) die "drive: bad shot name '$rest'" ;; esac
                ( name=$rest; args=; cmd_shot ) ;;
            *) die "drive: unknown verb '$verb' (wait|key|type|click|move|shot)" ;;
        esac
    done <"$1"
}

cmd_drive() {
    lane_up || die "lane $id is not running"
    set -- $args
    [ $# -eq 1 ] || die "drive takes one script file: game-session drive overlay.drive"
    run_drive "$1"
}
