#!/usr/bin/env bash
# TuxGT maintainer release: optional version bump, tag, draft GitHub release.
# A human publishes the draft on GitHub (drafts never auto-publish).
#
# Usage:
#   make release [BUMP=major|minor|patch|beta|promote-beta] [DRY_RUN=1]
#
# Without BUMP, drafts the current Cargo.toml version, failing if that
# version already has a release (or draft) on GitHub. With BUMP, bumps the
# workspace version, commits it, and drafts the new version:
#   major|minor|patch  normal bump; major/minor refused while a beta is in flight
#   beta               prerelease line: minor bump once on entry, then the beta
#                      counter (X.Y.Z-beta.N); patch while in flight hotfixes
#                      the last final instead
#   promote-beta       graduate the in-flight beta to its final
# Beta tags always draft with --prerelease, finals never do. DRY_RUN=1 prints
# every mutating step without running it (gh checks skipped).
set -eu

BUMP="${1:-}"
DRY_RUN="${DRY_RUN:-0}"
VERSION_FILE="src/tuxgt/Cargo.toml"
TARBALL="dist/tuxgt.tar.gz"

fail() { echo "error: $*" >&2; exit 1; }
run() {
    if [ "$DRY_RUN" = "1" ]; then
        echo "+ $*"
    else
        "$@"
    fi
}

[ -f "$VERSION_FILE" ] || fail "run from the repo root ($VERSION_FILE missing)"
[ -z "$(git status --porcelain)" ] || fail "working tree not clean (commit, stash, or remove changes first)"
[ "$(git branch --show-current)" = "master" ] || fail "not on master"
[ -f "$TARBALL" ] || fail "$TARBALL missing; run 'make package' first"
command -v cargo >/dev/null 2>&1 || fail "need 'cargo' installed"
if [ "$DRY_RUN" != "1" ]; then
    command -v gh >/dev/null 2>&1 || fail "need 'gh' installed (pacman -S github-cli) and authed"
    gh auth status >/dev/null 2>&1 || fail "'gh auth status' failed; run 'gh auth login'"
else
    echo "(dry-run: skipping gh checks)"
fi

CUR="$(grep -E '^version = "[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?"$' "$VERSION_FILE" | head -1 | cut -d'"' -f2)"
[ -n "$CUR" ] || fail "could not parse workspace version from $VERSION_FILE"
case "$CUR" in
    *-beta.*) BASE="${CUR%%-beta.*}"; BETA_N="${CUR##*-beta.}" ;;
    *) BASE="$CUR"; BETA_N="" ;;
esac
# Last final tag (betas excluded): hotfix base and final-release notes base.
LAST_FINAL="$(git tag --list 'v[0-9]*' --sort=v:refname | grep -v -- '-beta\.' | tail -1)"
# Beta bases with no final tag: lines still in flight (space-separated, else empty).
# The working tree alone can't prove this: a hotfix commits a final version
# while beta tags still exist.
IN_FLIGHT="$(git tag --list 'v[0-9]*-beta.*' | sed -E 's/-beta\.[0-9]+$//' | sort -u | while read -r b; do
    git rev-parse -q --verify "refs/tags/$b" >/dev/null || echo "$b"
done | tr '\n' ' ' | sed -E 's/^ +//;s/ +$//')"
TAG="v$CUR"
if [ -z "$BUMP" ]; then
    if [ "$DRY_RUN" != "1" ] && gh release view "$TAG" >/dev/null 2>&1; then
        fail "$TAG is already released; pass BUMP=major|minor|patch|beta|promote-beta"
    fi
    NEW="$CUR"
else
    major="${BASE%%.*}"; rest="${BASE#*.}"; minor="${rest%%.*}"; patch="${rest#*.}"
    case "$BUMP" in
        major|minor)
            [ -z "$BETA_N" ] || fail "beta $CUR in flight; ship it (BUMP=promote-beta) or hotfix stable (BUMP=patch) first"
            [ -z "$IN_FLIGHT" ] || fail "beta line(s) $IN_FLIGHT in flight; ship (BUMP=promote-beta) or hotfix stable (BUMP=patch) first"
            case "$BUMP" in
                major) NEW="$((major + 1)).0.0" ;;
                minor) NEW="$major.$((minor + 1)).0" ;;
            esac
            ;;
        patch)
            if [ -z "$BETA_N" ]; then
                NEW="$major.$minor.$((patch + 1))"
            else
                # Stable hotfix while a beta rides the next minor: patch the last final.
                [ -n "$LAST_FINAL" ] || fail "no final release to patch (only betas exist)"
                fbase="${LAST_FINAL#v}"
                fmajor="${fbase%%.*}"; frest="${fbase#*.}"; fminor="${frest%%.*}"; fpatch="${frest#*.}"
                NEW="$fmajor.$fminor.$((fpatch + 1))"
                [ -z "$(git tag --list "v$NEW-beta.*")" ] \
                    || fail "beta in flight for $NEW; ship it (BUMP=promote-beta) first"
            fi
            ;;
        beta)
            if [ -n "$BETA_N" ]; then
                NEW="$BASE-beta.$((BETA_N + 1))"
            elif [ -z "$IN_FLIGHT" ]; then
                NEW="$major.$((minor + 1)).0-beta.1"
            else
                # Post-hotfix: the one in-flight line resumes where it left off.
                case "$IN_FLIGHT" in *" "*) fail "multiple beta lines in flight ($IN_FLIGHT); resolve manually" ;; esac
                [ "v$major.$((minor + 1)).0" = "$IN_FLIGHT" ] \
                    || fail "beta line $IN_FLIGHT in flight but tree is $CUR; resolve manually"
                n_max="$(git tag --list "$IN_FLIGHT-beta.*" | sed -E 's/.*-beta\.([0-9]+)$/\1/' | sort -n | tail -1)"
                NEW="${IN_FLIGHT#v}-beta.$((n_max + 1))"
            fi
            ;;
        promote-beta)
            if [ -n "$BETA_N" ]; then
                NEW="$BASE"
            else
                case "$IN_FLIGHT" in
                    "") fail "not on a beta (current $CUR)" ;;
                    *" "*) fail "multiple beta lines in flight ($IN_FLIGHT); resolve manually" ;;
                esac
                NEW="${IN_FLIGHT#v}"
            fi
            ;;
        *) fail "BUMP must be major|minor|patch|beta|promote-beta (got '$BUMP')" ;;
    esac
    TAG="v$NEW"
    if git rev-parse "$TAG" >/dev/null 2>&1; then
        fail "tag $TAG already exists locally"
    fi
    if [ "$DRY_RUN" != "1" ] && gh release view "$TAG" >/dev/null 2>&1; then
        fail "$TAG is already released"
    fi
fi
# Betas always draft as prereleases, finals never do — no manual flag.
case "$TAG" in
    *-beta.*) PRERELEASE=1 ;;
    *) PRERELEASE=0 ;;
esac
echo "releasing TuxGT $TAG (BUMP=${BUMP:-none})"

# Release notes: shortlog since the previous tag for betas, since the last
# final for finals (so the graduated release notes the whole line, not just
# post-beta commits), or the full log + header on a first release.
notes_file="$(mktemp /tmp/tuxgt-release-notes.XXXXXX)"
trap 'rm -f "$notes_file"' EXIT INT TERM
PREV_TAG="$(git describe --tags --abbrev=0 2>/dev/null || true)"
case "$TAG" in
    *-beta.*) NOTES_BASE="$PREV_TAG" ;;
    *) NOTES_BASE="${LAST_FINAL:-$PREV_TAG}" ;;
esac
if [ -n "$NOTES_BASE" ]; then
    git log --format='- %s (%h)' "$NOTES_BASE..HEAD" >"$notes_file"
else
    {
        echo "Initial public release."
        echo ""
        git log --format='- %s (%h)'
    } >"$notes_file"
fi
[ -s "$notes_file" ] || fail "empty release notes ($NOTES_BASE..HEAD has no commits)"
echo "--- notes ---"; cat "$notes_file"; echo "---"

if [ -n "$BUMP" ]; then
    run sed -i -E "s/^version = \"[0-9]+\\.[0-9]+\\.[0-9]+(-beta\\.[0-9]+)?\"$/version = \"$NEW\"/" "$VERSION_FILE"
    # Re-sync the lockfile member versions offline (no dependency changes).
    run cargo metadata --format-version 1 --offline --manifest-path src/tuxgt/Cargo.toml >/dev/null
    if [ "$DRY_RUN" != "1" ]; then
        grep -A1 'name = "tuxgt"$' src/tuxgt/Cargo.lock | grep -q "version = \"$NEW\"" \
            || fail "lockfile did not pick up $NEW"
    fi
    run git add "$VERSION_FILE" src/tuxgt/Cargo.lock
    run git commit -m "Release $TAG"
fi

run git tag -a "$TAG" -m "TuxGT $TAG"
run git push origin master "$TAG"

if [ "$PRERELEASE" = "1" ]; then
    run gh release create "$TAG" "$TARBALL" --title "TuxGT $TAG" --draft --prerelease --notes-file "$notes_file"
else
    run gh release create "$TAG" "$TARBALL" --title "TuxGT $TAG" --draft --notes-file "$notes_file"
fi
run gh release view "$TAG" --json url,tagName -q '.url'
