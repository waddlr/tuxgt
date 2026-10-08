# Build

Reproducible release builds. Code: `Makefile`, `.github/release.sh`, `src/tuxgt/tuxgt-app/build.rs`. Surfaces: `release.tarball` (Exists), `build.cpu-baseline` (Exists), `build.proxy` (Exists).

## Rules

- CPU baseline: every shipped binary pins the x86-64 baseline. Rust: `-C target-cpu=x86-64` on the Makefile cargo line plus `.cargo/config.toml` for bare cargo, gated by `make check-baseline`. C: `-march=x86-64 -mtune=generic` on both gcc launcher lines (64- and 32-bit) and the mingw proxy line.
- UTC only in build meta: embedded instants are UTC. UI/CLI renders them in the user's offset at display time, never at embed time.
- `SOURCE_DATE_EPOCH`: release builds export it from the version-commit time (`git log -1 --format=%ct` after the bump). The embedded build instant is the SDE instant (vergen honors SDE for `build_date`/`build_timestamp`), so tag rebuilds are identical.
- Tarball: `dist/tuxgt.tar.gz` is byte-reproducible: `--sort=name`, `--owner=0 --group=0`, `--mtime=@$SOURCE_DATE_EPOCH`, `gzip -n`.
- Proxy PE: the mingw link passes `-Wl,--no-insert-timestamp`; the DLL carries no wall-clock TimeDateStamp.
