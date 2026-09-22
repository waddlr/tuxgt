# Launch

## Play (does not require store mutation)

TuxGT never manages a store-owned game process. Steam and Heroic rows dispatch to their clients; only manual rows exec.

- Steam: `steam steam://rungameid/<CGameID>`. Owned app: CGameID is the appid. Standalone shortcut: `(shortcuts.vdf u32 << 32) | 0x02000000`. Steam starts the **store** target. Detected exe is doctor-only. Do not exec the PE. Launch options / Proton stay Steam’s. Do not wrap `tuxgt-launcher` around `steam steam://`.
- Heroic: `heroic "heroic://launch?appName=<game_id>&runner=<gog|sideload|legendary|nile>"` (`sideload` is Heroic's wire name for our `standalone` rows; runner omitted when the store is none of those). Heroic applies its stored Wine/Proton, wrappers, and env. Do not exec the game PE. Direct `legendary` / `gogdl` launch is **never**.
- Manual: we exec the store (or override) exe; compose wrappers; wrap `tuxgt-launcher`. This is the only owned path (spawn-and-wait harvest applies here).
- Compose, don’t clobber: if existing MangoHud/`gamemoderun` is in the store config, include it on paths we exec ourselves.

**Launch Mode** (store rows, exclusive): one channel armed. Handle is **not** a master switch over Apply.

| Mode | Store config | Session | Who loads `.so` |
|---|---|---|---|
| **Hook protonfixes** | untouched | `inject=1` | protonfixes correlator + conf (GE/Cachy only) |
| **Update Launch Options** | trampoline in Steam LaunchOptions / Heroic wrapperOptions (compose with existing, incl. `linuwux`) | `inject=0` (hook must no-op) | trampoline (self-arms from argv, even with `inject=0`) |
| **Not hooked** | trampoline restored away if we wrote it | `inject=0` | nobody |

**Forbidden:** Hook + Apply together. `inject=` means the hook channel only. `WRAPPERS=` stays trampoline-only (hook never wraps gamescope/`gamemoderun`). Manual rows have no store Apply and stay out of this radio; owned Play still wraps `tuxgt-launcher` as today.

Mutual exclusion: selecting **Update Launch Options** → `set_handle(false)` then `apply_launch`; selecting **Hook** → `restore_launch` if applied, then `set_handle(true)`; selecting **Not hooked** → restore if applied + handle off. Apply while Handle on clears Handle, then Applies (CLI `tuxgt launch --apply` same). `tuxgt games handle --on` while applied restores the trampoline first. Client restart still required after Apply/Restore (Heroic caches GamesConfig; Steam `localconfig.vdf`).

Protocol argv still cannot carry env. The session file is the execute-time channel for env, including enabled globals.

## Apply to Steam / Heroic (optional)

Persist **one** wrapper (`tuxgt-launcher`) so the store's own Play button runs the trampoline after a **client restart** (Heroic caches `GameConfig.config`; Steam owns `localconfig.vdf`). The trampoline self-arms from argv: when it runs it applies session env + outer wrappers (gamescope / `gamemoderun`) + `.so`, ignoring `inject=` for passthrough (correlator miss still loads `.so`). The hook still returns on `inject!=1`. Do not write knobs or `TUXGT_*` into Heroic `enviromentOptions`. Restore removes the wrapper; it does not flip handle.

In-game ReShade on that Apply path (install + enable ReShade, restart the client, Play from Steam/Heroic or from TuxGT) is **verified**. Handle-only GE/Cachy (no Apply) is verified the same way (`inject.ingame-proof`). GUI live-verify does not count as this proof.

Handle-on GE/Cachy does **not** need Apply. Apply is for outer argv wrappers, Wine, native, Valve Proton, and `PRESSURE_VESSEL_FILESYSTEMS_RW` if overlay-only `LD_PRELOAD` fails in SLR.

Handle off + Vanilla + a needed channel (preload, env, or argv wrappers) → GUI primary **Enable & Play**. Arms **Update Launch Options when argv wrappers are on**, else Hook if GE/Cachy else Update Launch Options, then dispatches; no extra store write on the Hook path. **Apply & Play** is no longer the primary for “I want mods.” Already Hooked or Applied: Play only.

## Needs (`launch.needs`)

Per enabled instance + per-game env/custom + wrappers. Mixed preload+install is Preload. Argv wrappers = gamescope / GameMode / MangoHud-as-argv (hook cannot wrap them). **Not hooked is always legal**: it pauses injection and leaves mods, knobs, and wrappers untouched.

| Needs | Hook (GE/Cachy) | Update Launch Options | Not hooked |
|---|---|---|---|
| Argv wrappers | no | required | legal (paused) |
| Preload or Env, no argv wrappers | yes | yes | legal (paused) |
| Install-only (all `install`, no env/wrappers/preload) | not needed | not needed | enough |
| Nothing TuxGT | hidden | hidden | default |

Illegal arm: no-op, control disabled with one reason. Enabling an argv wrapper while Hook is armed auto-switches to Update Launch Options. Install-only: collapse the radio to one muted line, Not hooked enabled.

**Auto-restore:** needs dropping to none while Hook or Update Launch Options is armed (last mod uninstalled, last knob cleared, wrappers off) restores the trampoline and drops the handle in the same write, so the radio repaints Not hooked. An arm nothing needs never persists — `tuxgt games handle <id> --on` on a needs-free game is cleared by that same write and reports `off`. A mode never changes mod/knob management state. Vanilla store Play does not inject env or preload.

## Adapters and plans

Default **adapter**: preload (proven in-game for stock-named ReShade and `dxgi`-slot OptiScaler).

Plans are shown even when disabled (with a reason):

| Plan | When valid | Default |
|---|---|---|
| Preload (`tuxgt-launcher` + `LoadDLL=`/`IncludeFile=`) | Any injector; ReShade dests stock-named, OptiScaler on `dxgi` | Yes, default |
| Install proxy DLL + `WINEDLLOVERRIDES` | Fallback when preload is unproven for a package; slot picker. A proxy slot always adds `<stem>=n,b` on Play and in the session; `<self>` does not | When preload invalid |
| `PROTON_USE_OPTISCALER=1` | Official recipe lists `proton_env` **and** the game’s Proton is CachyOS/GE | No shipped OptiScaler recipe lists it. Hidden for custom/fork instances |

Conflicts: dest proxy names are the slots — auto-select a free slot / preload stock-named ReShade + preload OptiScaler on `dxgi`. Each of those preloads works in game alone. Together, OptiScaler works and stock-named ReShade does not (`KNOWN_ISSUES.md`). **Show the choice.**

Foreign DLL: refuse DXVK/vkd3d/ENB/SpecialK overwrite without confirm.

32-bit is detected and stored. DirectX 9/10/11/12 **plumbing** is supported (selection, `Type=dx9_32`/`dx9_64`/`dx10_32`/`dx10_64`/`dx11_32`/`dx11_64`/`dx12_32`/`dx12_64`, `ReShade32.dll` arch-32 payload, `libtuxgt-launcher32.so` via `gcc -m32`, staging, `DownloadUrl32` parsing). Session `TUXGT_LAUNCHER_SO` is the matching primary; the hook and trampoline also `LD_PRELOAD` the sibling ELF class when it sits next to that file (WoW64: 64-bit Unix wine, 32-bit PE). The i386 gate exists (`src/launcher/gate.c` in the `-m32` SO, `launch.32bit-in-game-gate`). In-game DX9/DX10 proof is verified. For 32-bit Wine, enable the `proton-wow64` knob (GE/CachyOS). Vulkan and XR remain out of scope.

Engineering: `docs/dev/app/plugin/launch-play.md`, `docs/dev/app/plugin/launch-adapter.md`, `docs/dev/app/plugin/apply.md`.
