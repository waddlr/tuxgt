#!/bin/sh
# game-session report: render a bundle as a human proof page.
# Sourced by src/tuxgt/tools/game-session; functions only.
#
# Input is a test bundle dir (frames-<scenario>/, <scenario>-*.mkv,
# result.txt). An optional bundle-local report.tsv pins the exact frame
# and caption per scenario, one tab-separated row per scenario:
#   scenario <TAB> frame.png (bundle-relative) <TAB> caption <TAB> video.mkv
# Without it, every frames-*/ dir contributes its middle frame with the
# scenario name as caption. Output: <bundle>/report/index.html plus the
# picked frames, a contact sheet, and short mp4 clips beside it.

esc() { printf '%s' "$1" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g'; }

report_entries() {
    # $1=bundle $2=out-file; rows: scenario TAB frame TAB caption TAB video
    if [ -f "$1/report.tsv" ]; then
        grep -v '^#' "$1/report.tsv" | grep -v '^$' >"$2" || true
        return 0
    fi
    : >"$2"
    for d in "$1"/frames-*/; do
        [ -d "$d" ] || continue
        s=${d%/}; s=${s##*/}; s=${s#frames-}
        cnt=$(ls "$d"/*.png 2>/dev/null | wc -l)
        [ "$cnt" -gt 0 ] || continue
        f=$(ls "$d"/*.png | sed -n "$((cnt / 2 + 1))p")
        v=$(ls "$1/$s"-*.mkv 2>/dev/null | head -1)
        printf '%s\t%s\t%s\t%s\n' "$s" "${f#"$1"/}" "$s" "${v#"$1"/}" >>"$2"
    done
}

cmd_report() {
    set -- $args
    [ $# -eq 1 ] || die "report takes one BUNDLE_DIR"
    [ -d "$1" ] || die "bundle dir not found: $1"
    bundle=$(CDPATH= cd -- "$1" && pwd)
    out=$bundle/report
    mkdir -p "$out"
    raw=$out/.entries.raw
    entries=$out/.entries.tsv
    report_entries "$bundle" "$raw"
    [ -s "$raw" ] || die "no scenarios in $bundle (need frames-*/ dirs or report.tsv)"
    have264=0
    ffmpeg -hide_banner -encoders 2>/dev/null | grep -q ' libx264 ' && have264=1
    : >"$entries"
    while IFS="$(printf '\t')" read -r s frame caption video || [ -n "$s" ]; do
        [ -n "$s" ] || continue
        case $s in *[!A-Za-z0-9_-]*) die "report: bad scenario name '$s'" ;; esac
        case $frame in /*|../*|*/../*|*/..) die "report: frame escapes the bundle: $frame" ;; esac
        case $video in /*|../*|*/../*|*/..) die "report: video escapes the bundle: $video" ;; esac
        [ -f "$bundle/$frame" ] || die "report: frame missing for $s: $bundle/$frame"
        cp "$bundle/$frame" "$out/$s.png"
        mp4=
        if [ -n "$video" ] && [ -f "$bundle/$video" ] && [ $have264 -eq 1 ]; then
            if ffmpeg -y -v error -ss 1 -t 4 -i "$bundle/$video" -an -c:v libx264 \
                    -pix_fmt yuv420p -movflags +faststart "$out/$s.mp4" && [ -s "$out/$s.mp4" ]; then
                mp4=$s.mp4
            else
                rm -f "$out/$s.mp4"
                printf 'game-session: report: clip for %s skipped (transcode failed)\n' "$s" >&2
            fi
        fi
        printf '%s\t%s\t%s\t%s\n' "$s" "$s.png" "${caption:-$s}" "$mp4" >>"$entries"
    done <"$raw"
    n=$(wc -l <"$entries")
    [ "$n" -gt 0 ] || die "report: no usable scenarios in $bundle"

    # Contact sheet: 2-column stack of the picked frames, all scaled to
    # the first frame's size; an odd count repeats the first frame.
    first=$(sed -n '1p' "$entries" | cut -f2)
    wh=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height \
        -of csv=p=0 "$out/$first") || die "report: cannot probe $first"
    fw=${wh%,*}; fh=${wh#*,}
    case $fw in ''|0) die "report: $first is not a readable image" ;; esac
    inputs=; i=0
    while IFS="$(printf '\t')" read -r s png _c _m || [ -n "$s" ]; do
        [ -n "$s" ] || continue
        inputs="$inputs -i $out/$png"; i=$((i + 1))
    done <"$entries"
    total=$i
    if [ $((total % 2)) -eq 1 ] && [ $total -gt 1 ]; then
        inputs="$inputs -i $out/$first"; total=$((total + 1))
    fi
    if [ $n -eq 1 ]; then
        cp "$out/$first" "$out/contact-sheet.png"
    else
        rows=$((total / 2)); filter=; i=0
        while [ $i -lt $total ]; do
            filter="$filter[$i:v]scale=$fw:$fh[s$i];"; i=$((i + 1))
        done
        r=0
        while [ $r -lt $rows ]; do
            filter="$filter[s$((r * 2))][s$((r * 2 + 1))]hstack[row$r];"; r=$((r + 1))
        done
        if [ $rows -eq 1 ]; then
            filter="${filter}[row0]null[out]"
        else
            stack=; r=0
            while [ $r -lt $rows ]; do stack="$stack[row$r]"; r=$((r + 1)); done
            filter="${filter}${stack}vstack=inputs=$rows[out]"
        fi
        # shellcheck disable=SC2086
        ffmpeg -y -v error $inputs -filter_complex "$filter" -map '[out]' \
            "$out/contact-sheet.png" || die "report: contact sheet failed"
    fi

    status=INCOMPLETE; sclass=todo
    if [ -f "$bundle/result.txt" ]; then
        # FAIL-first: a PASS line must never outrank a FAIL line.
        if grep -q '^FAIL' "$bundle/result.txt"; then status=FAIL; sclass=fail
        elif grep -q '^PASS' "$bundle/result.txt"; then status=PASS; sclass=pass; fi
    fi
    title="game-session proof report — $(basename "$bundle")"
    {
        printf '<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n'
        printf '<title>%s</title>\n<style>\n' "$(esc "$title")"
        cat <<'CSS'
body { background:#141414; color:#e8e8e8; font-family: ui-monospace, "DejaVu Sans Mono", monospace; margin:2rem auto; max-width:1100px; padding:0 1rem; line-height:1.5; }
h1 { font-size:1.3rem; } h2 { font-size:1.05rem; margin-top:2rem; }
.pass { color:#7ee787; font-weight:bold; } .fail { color:#ff7b72; font-weight:bold; } .todo { color:#e3b341; font-weight:bold; }
.grid { display:grid; grid-template-columns:1fr 1fr; gap:1rem; }
figure { margin:0; } img, video { width:100%; display:block; border:1px solid #444; }
figcaption { padding:.4rem 0; font-size:.9rem; }
pre { background:#1d1d1d; padding:1rem; overflow:auto; font-size:.8rem; }
CSS
        printf '</style>\n</head>\n<body>\n'
        printf '<h1>%s</h1>\n' "$(esc "$title")"
        printf '<p>Status: <span class="%s">%s</span> — bundle <code>%s</code>.</p>\n' \
            "$sclass" "$status" "$(esc "$bundle")"
        printf '<p>Frames are compositor captures from a headless gamescope lane (no window on the operator desktop). Captions state what each scenario proves; the oracle log below is the machine verdict behind them.</p>\n'
        printf '<h2>Contact sheet</h2>\n<img src="contact-sheet.png" alt="Contact sheet of scenario frames">\n'
        printf '<h2>Scenarios</h2>\n<div class="grid">\n'
        while IFS="$(printf '\t')" read -r s png caption mp4 || [ -n "$s" ]; do
            [ -n "$s" ] || continue
            printf '<figure><img src="%s" alt="%s frame"><figcaption><strong>%s</strong> — %s</figcaption>' \
                "$(esc "$png")" "$(esc "$s")" "$(esc "$s")" "$(esc "$caption")"
            [ -n "$mp4" ] && printf '<video src="%s" controls loop muted></video>' "$(esc "$mp4")"
            printf '</figure>\n'
        done <"$entries"
        printf '</div>\n'
        if [ -f "$bundle/result.txt" ]; then
            printf '<h2>Oracle log (result.txt)</h2>\n<pre>%s</pre>\n' "$(esc "$(cat "$bundle/result.txt")")"
        fi
        if [ -f "$bundle/versions.txt" ]; then
            printf '<h2>Versions</h2>\n<pre>%s</pre>\n' "$(esc "$(cat "$bundle/versions.txt")")"
        fi
        printf '</body>\n</html>\n'
    } >"$out/index.html" || die "report: cannot write $out/index.html"
    rm -f "$raw" "$entries"
    printf 'report\t%s\n' "$out/index.html"
}
