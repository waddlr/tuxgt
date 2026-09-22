# Known issues (development)

Player-facing bugs are in the repo-root [KNOWN_ISSUES.md](../../KNOWN_ISSUES.md). This file is for problems that show up in the lab, not on a normal desktop.

## Headless lane panics when the app is started again

**What happens:** In a headless GUI lane (`src/tuxgt/tools/gui-session`), killing `tuxgt` and starting it again in the same lane panics in the Wayland event loop (`client.rs` unwrap on `None` keymap state — Modifiers arrived before a valid keymap). The window never appears. A fresh start of the whole lane on the same prefix opens.

**Why:** The lane seat uses a wlroots `wlinput` virtual keyboard whose compositor-side state survives the app process. Real desktops supply a keymap, so this does not happen there.

**Workaround:** Stop and start the whole lane instead of relaunching the app inside it. CLI, core, and real game sessions are unaffected.
