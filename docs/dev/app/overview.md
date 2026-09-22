# App docs

Rust desktop + CLI (`src/tuxgt/`). Product rules, GUI spec, and plugin contracts are **separate audiences** under this folder. Open one file.

| Dir | Job | Open when |
|---|---|---|
| `core/` | Product rules (what the app is, ids, launch, install, stack) | Changing a product rule |
| `gui/` | Visual/IA + gpui landmines | Changing windows/widgets |
| `plugin/` | Host engineering contracts | Changing a capability trait/CLI |

CLI is clap over `tuxgt-core` (same operations as the GUI). No `cli/` spec.

On product conflict, `core/` wins — stop and fix the other file.

As-built state: `docs/agent/MAP.md`. Open work: `.agents/docs/TASKS.md`. GUI lane: `docs/agent/landmines.md`. Lab-only bugs: `docs/dev/known-issues.md`. Player-facing bugs: repo-root `KNOWN_ISSUES.md`.
