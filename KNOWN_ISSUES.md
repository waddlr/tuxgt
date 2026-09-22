# Known issues

Open bugs. Setup problems that are not bugs are in [docs/troubleshooting.md](docs/troubleshooting.md).

## ReShade does not load when OptiScaler is also enabled

**What happens:** Stock-named ReShade (`ReShade64.dll` / `ReShade32.dll`) works in a game by itself. OptiScaler on the `dxgi` slot works by itself. With both enabled on preload, OptiScaler works and ReShade does not.

**Status:** Reproduced in game. Cause not found yet.

**Workaround:** Use one of them. There is no known way to run both together.

## Bitness and API recipe gates are not enforced

**What happens:** A recipe can declare `arch` (`32` / `64`) or `api` on `[[payload]]`. TuxGT does not use those fields to hide a mod or refuse install. The Game → Mods picker still lists the mod, and install proceeds. OptiScaler is 64-bit only and still installs on a 32-bit game.

**Status:** Open. Catalog and install do not treat payload `arch` / `api` as restrictions.

**Workaround:** Install OptiScaler and other 64-bit-only packs only on 64-bit games. The game's bitness is on its About tab.
