# Agent guidelines

## Ask vs do

Default is **ask**. Repo writes need a **command word**: implement, fix, do, apply, land.

|User said|Agent does|
|---|---|
|question, "what's missing", "can we add", "what's wrong", review, explain|read-only. Answer. No file edits.|
|"plan …" / plan mode|write the plan file only.|
|command word + scope ("implement managed-tooltip-silent", "fix the confirm dialog", "do window-open-no-fallback")|after MAP/dep gate, edit that scope only.|
|named a task id **without** a command word|do not start it.|
|ambiguous|one question, then stop.|

Finding a gap or skipping work: **add** a `.agents/docs/TASKS.md` row (and a `docs/agent/MAP.md` Missing row if the surface is new). Do not wait for the user to say "add". Do not start that work without a command word.

MAP edits that change a surface state are a plan step even outside plan mode: show the delta, wait, land MAP before implementation of that surface.

## Git

- NEVER `git push`. Pushing is always human-driven. Leave the work committed or dirty; the user pushes.
- Commit messages are the subject, plus a body only when the subject is not enough. Never add `Co-authored-by`, `Generated-by`, `Signed-off-by`, or any trailer or footer that names an assistant, model, or coding tool. This includes Claude, Cursor, Copilot, and Grok. Do not keep one that a hook or a template inserted. Strip it before the commit is created, or by amending the task branch while that commit is not on master.
- `git commit --amend` is allowed on the task branch, in its worktree, when `HEAD` is not on `master`. Never amend a commit that is on `master`, including the landing commit after the fast-forward. Never amend from the master checkout.
- Every task with a command word runs on its own branch in its own worktree under `.worktrees/<slug>/`, never in the master workspace. Master workspace stays clean for human use.
  - `git worktree add .worktrees/<slug> -b <branch>` off master. `<slug>` = task id or short scope slug (e.g. `fix-confirm-dialog`).
  - All writes, commits, and verification for that task happen in that worktree. Do not run them from the master checkout.
  - Removed on landing (see Landing). A landed slug is never reused. A follow-up gets a new slug and a new branch off current master.
  - Exception only: the human explicitly says to use the master workspace for that task.

## Components (real paths)

|Component|Path|Docs|
|---|---|---|
|launcher|`src/launcher/`|`docs/dev/launcher/overview.md`|
|app|`src/tuxgt/`|`docs/dev/app/`|
|protonfixes hook|`src/protonfixes-hook/`|`docs/dev/app/core/install.md`|

Do not conflate.

## Filesystem

- Write / edit / delete / create: ONLY inside the repository root and `/tmp/` (temp files).
- `external/`: read-only. Never write to it. Never compile it, vendor it, or copy it into `src/` or the `Makefile`.
- Anything outside those paths: forbidden, except the live prefix (`$HOME/tuxgt` by default, or `TUXGT_DATA` from `~/.config/tuxgt.conf`) refreshed via `make deploy`, and the packaged `tuxgt install` outputs (PATH symlinks, KDE desktop, boot conf).
- Game dir: never written by us except the install adapter (explicit dest copies). Stock `ReShade64.dll` + `*.addon64` stay outside the game dir unless that adapter runs.
- Allowed doesn't mean do it. Don't go looking outside the project dir without cause.

## Injector launcher

- One preload only: `build/libtuxgt-launcher.so` from `src/launcher/*.c` (pure C, `gcc -shared -fPIC`).
- Per-game config is injector-agnostic: `LoadDLL` (staged, then loaded in order) + `IncludeFile` (staged only); `Type` is informational. No injector-specific keys; single ReShade dest-basename quirk in `docs/dev/launcher/overview.md`.

## File size

Target ≤300 lines per file, hard cap 400 (including in-file tests). Split at existing `fn` / `impl` / `struct` boundaries into a directory module (`foo.rs` → `foo/mod.rs` + siblings). Do not add traits, types, crates, or helpers to shrink a file. Do not split a function across files. Data tables (`env/knobs.rs` `KNOBS`, theme palette literals) may exceed the cap. Tests move with the sibling they cover. Crate-root `pub use` names stay. `pub(crate)` only when a sibling needs the item.

## Ground truth

Code first. Before claiming exists / missing / can-add / needed:

1. Open the **code path** (`docs/agent/MAP.md` Code column tells you where). Code wins; say MAP is stale on disagreement.
2. MAP is the index, not proof. Specs are rules, not proof.
3. Open **one** spec only when you need the rule behind a behavior.
4. `Later` and `Never` are hard stops unless the user reopens them.

`.agents/docs/INDEX.md` is a folder catalog, not a summary of reality. Open INDEX → folder overview → one spec. Do not dump the tree into context.

## Contradiction / Needs gate

Refuse work on a surface when:

- MAP and code disagree for that surface or a **needed function**, or two **live** specs disagree. Resolve MAP/spec first (approved chore).
- A needed function is Missing, or Partial *in the part this work calls* (GUI mods-install needs `download.acquire`; if acquire is Missing/conflicted, GUI install does not start).

Refuse by naming the blocking MAP row.

Validate / Done-when may only name functions MAP marks Exists, or that **this same authorized work** implements earlier.

## Spec → MAP

New surface in a spec → MAP row **Missing** (or Partial if some code already exists) in the same change. You approve. Land it before coding. Do not add a spec with no MAP row.

## Local notes

`.agents/` is gitignored. These paths are the workflow slots. A clone does not include another person's files. Create a file when you use that slot.

|Path|Job|
|---|---|
|`.agents/docs/TASKS.md`|Open work|
|`.agents/docs/plans/`|Implementation packets. A packet is not a command to start.|
|`.agents/docs/roadmap/`|Roadmap packets. Not startable.|
|`.agents/docs/NEW_ROADMAP.md`|Roadmap source list|
|`.agents/docs/INDEX.md`|Folder catalog|
|`.agents/HISTORY.md`|Landed-work log|
|`.agents/skills/`|Local skills|

Published agent docs are `docs/agent/MAP.md` and `docs/agent/landmines.md`. Product rules are `docs/dev/`.

## Tasks

Do not start a TASKS Work row unless the user named that id **and** used a command word. Roadmap table is not startable.

Landing: append each landed Work row to `.agents/HISTORY.md` as a landed line and drop it from `.agents/docs/TASKS.md` after the fast-forward. Not inside the landing commit. No extra command word needed.

GUI crate lock: `docs/dev/app/core/overview.md` recorded decision must be `gpui-kit`.

## Behavior

- Stay within scope. Do what was asked.
- No paraphrasing, no filler, no unnecessary words. Do not repeat yourself. Do not overdesign.
- Ask the user before renaming anything project-level and externally visible (binaries, files, env vars, config keys). Internal names are the agent's call.
- Do not fall into trial-and-error: understand first, then act.
- Do not presume you know the answer. Look at the code and the one spec before claiming.

## Done

A task is complete only after an independent review-fix loop:

1. Implementer validates: the task's tests and Done-when, actually run.
2. A separate reviewer checks quality, correctness, and slop. The implementer never self-accepts.
3. Implementer applies the findings; the reviewer re-verifies the fixes.
4. Clean review → the user accepts and lands.

## Landing

Landing to master = Done. One task branch becomes exactly one new commit on master, then the branch is deleted. Never amend a master commit. Never push. Never reuse the landed branch name.

Run every git and cargo command for the branch from `.worktrees/<slug>/`. Do not run them from the master checkout.

Stop, and do not stash, if the master worktree is dirty. Stop if a rebase conflicts until the conflict is resolved in the worktree. Never create a merge commit.

1. Rebase. In the worktree, `git rebase master`. This is a no-op when the branch is already on the master tip. On conflict, resolve and continue the rebase. Do not fast-forward.
2. Format. From `src/tuxgt` in the worktree, `cargo fmt --all`. Leave that diff uncommitted until the squash. Do not format the master checkout.
3. Squash to one commit whose parent is the current master tip. Include every task commit, the format diff, and any still-uncommitted task files. Unrelated dirt stops the land. Stage the format diff and those task files (do not `git add -A`), then `git reset --soft master` and one `git commit`. That is a new commit, not an amend of master, and not an interactive rebase. Amending the task-branch tip is still allowed while that commit is not on master. The subject is one sentence for the task outcome. Do not paste the squashed subjects into the message. No assistant trailer (see Git). Before continuing, `HEAD^` must be the master tip.
4. Prove that commit. Re-run the task's tests on it. On failure, fix on the task branch (amending that unlanded commit is allowed) and repeat from step 2. Do not fast-forward.
5. Fast-forward. If master has moved since step 1, repeat from step 1. In the master workspace, `git merge --ff-only <branch>`.
6. Delete the worktree and the branch: `git worktree remove .worktrees/<slug>`, then delete the branch. Do not create that branch name again.

The landing commit includes the tracked files the task changed: code, `docs/agent/MAP.md`, `docs/agent/landmines.md`, and any `docs/dev/` spec. MAP states in that commit match the landed code. No Missing or Partial remains where that code now Exists.

`.agents/` is gitignored. Update it after the fast-forward, in the master workspace, outside the landing commit:

- `.agents/docs/TASKS.md`: drop the landed Work row. No row claims pending for landed work.
- `.agents/HISTORY.md`: append the landed line (per Tasks).
- Scratch removed: task's `/tmp` files, fixtures, stray leftovers deleted.
- Master clean: `git status` shows none of the landing's dirt; no build artifacts committed. `git worktree list` shows master plus only other live tasks' worktrees.
- Verify before yielding: `git worktree list`, `git status`, and the touched TASKS/MAP rows. Master gained exactly one commit, and that commit's parent is the previous master tip.

## Changes

Golden Rule: the best change is the one with the least changed content.

- Only lines required by the task were touched.
- No reformatting, reordering, or "while I'm here" edits. Landing step 2 is the only `cargo fmt --all`, and its diff goes into the one landing commit.
- No dead code, commented-out code, or leftover debug artifacts.
- Style matches surrounding code.
