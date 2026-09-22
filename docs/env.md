# Env

Env options are per-game environment settings: named **knobs** (Proton, Wine, DXVK, VKD3D, Mesa, and GPU-vendor presets) plus freeform **custom `KEY=VALUE` pairs**. They apply on the next Play — no reinstall, no store change.

## Where to find them

- **Game → Environment tab** — knobs for the selected game, grouped (Proton / Wine, DXVK, VKD3D, Mesa, NVIDIA, AMD, Intel, Other), plus a **Custom environment** section.
- **Settings → Game Env** — app-wide defaults for games TuxGT injects.
- **Advance** — a collapsed section on both pages holding knobs that do not apply here: GPU-vendor groups for hardware you don't have, DXVK on DX12 games (and VKD3D on other APIs), and GE/CachyOS-only knobs when the game's Proton is neither. A knob you set always stays in the main list.

## How values resolve

- A per-game value wins over the global default. An unset knob writes no env at all.
- Each knob row has a switch: on stores the value, off stops applying it (the game falls back to the global default when one is set). Multi-value knobs also have a value picker; freeform knobs (for example `wine-dlloverrides`) take typed text.
- Knobs are scoped: Proton-only, Proton+Wine, or any game. Some are further tagged GE/CachyOS-only — those sit in Advance unless the game's Proton is GE or CachyOS.
- **Unmanaged** rows show values set outside TuxGT (your shell or session) and cannot be edited here. Setting a global is refused while the outside environment already defines that knob.

## Custom environment

Per-game `KEY=VALUE` pairs, always on, applied alongside knobs. In the GUI: **Custom environment → Add KEY=VALUE**, one pair per row with **Remove** per row. Keys must look like `FOO_BAR` (letters, digits, underscore; must not start with a digit).

## Mod-shipped env rows

Some mods ship env vars with the recipe (for example `d3dcompiler-47`'s `WINEDLLOVERRIDES`). Those appear as toggle rows on the installed card — enable or disable per game.

## In the terminal

```sh
tuxgt env list                        # all knobs with scope + help
tuxgt env list steam::814380          # applicable knobs + values for one game
tuxgt env get steam::814380 dxvk-hud
tuxgt env set steam::814380 dxvk-hud full
tuxgt env set steam::814380 proton-no-fsync   # single-value knobs: value optional
tuxgt env disable steam::814380 dxvk-hud      # keeps value, falls back to global
tuxgt env enable steam::814380 dxvk-hud
tuxgt env unset steam::814380 dxvk-hud        # clears the stored value

tuxgt env global list
tuxgt env global set dxvk-hud full
tuxgt env global unset dxvk-hud

tuxgt env custom list steam::814380
tuxgt env custom add steam::814380 FOO=bar
tuxgt env custom remove steam::814380 FOO

tuxgt instance env steam::814380 d3dcompiler-47              # list mod-shipped rows
tuxgt instance env steam::814380 d3dcompiler-47 disable WINEDLLOVERRIDES
```
