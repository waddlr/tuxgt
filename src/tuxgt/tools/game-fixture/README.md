# game-fixture — deterministic D3D11 fixture game

Minimal Windows PE (x86_64, D3D11) used by the `game-e2e-visual` lane
as a synthetic "game" with a fully documented, stable scene. Source
only: build on demand, no binary is committed (`build/` is output).

```sh
src/tuxgt/tools/game-fixture/build.sh [OUTDIR]   # → tuxgt-fixture-d3d11.exe
```

Requires `mingw-w64` (`x86_64-w64-mingw32-g++`; override `MINGW_CXX`).
Run under Proton, e.g. in a `game-session` lane, or directly:

```sh
STEAM_COMPAT_DATA_PATH=<scratch> STEAM_COMPAT_CLIENT_INSTALL_PATH=~/.steam/root \
  <proton-dir>/proton run tuxgt-fixture-d3d11.exe [--animate] [--seconds N]
```

The fixture writes `fixture.log` beside the exe; `fixture first-frame`
is the observable event tests anchor observation windows to.

## Scene / region map (keep in sync with fixture.cpp)

Backbuffer is 640×360, `B8G8R8A8_UNORM`. Each frame is a full
background clear plus five solid quads drawn with one tiny shader pair
(compiled at startup via the prefix's `d3dcompiler_47.dll`; D3D11
clears ignore scissor rects, so regions are drawn, not cleared).
`--readback` logs the actual backbuffer pixels at frame 90 for
render-vs-present diagnosis:

| Region | Rect (l,t,r,b) | Color (R,G,B) |
|---|---|---|
| Background | 0,0,640,360 | (32,32,32) |
| Quadrant TL | 0,0,320,180 | (200,32,32) red |
| Quadrant TR | 320,0,640,180 | (32,200,32) green |
| Quadrant BL | 0,180,320,360 | (32,32,200) blue |
| Quadrant BR | 320,180,640,360 | (200,200,32) yellow |
| Center marker | 16×16 at (320,180) | (240,240,240) |

Assertion sample rects (64×36, centered in each quadrant, inset from
edges and the center marker): TL (128,72), TR (448,72), BL (128,252),
BR (448,252). Overlay banners (ReShade) draw in the top strip, so TL/TR
samples can be covered while a banner is up; effect A/B assertions
should prefer BL/BR, and banner assertions own the top strip.

`--animate` rotates the quadrant colors every 15 frames (quadrant *q*
shows color *(q+rot) mod 4*, rot increments per step) and sweeps the
center marker ±120px horizontally at
2px/frame. Present is vsync-paced (`Present(1, 0)`), so at the lane's
30 Hz the rotation period is 0.5 s. `--seconds N` exits with code 0
after N seconds.
