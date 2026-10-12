BUILD ?= build
# Build flavor: dev (fast debug) | trace (release opts + line tables) | release (stripped, LTO).
# prepare/package/deploy inherit TYPE from the command line; release always forces TYPE=release.
TYPE ?= dev

ifeq ($(TYPE),dev)
CARGO_ARGS :=
TARGET_SUBDIR := debug
else ifeq ($(TYPE),trace)
CARGO_ARGS := --profile release-trace
TARGET_SUBDIR := release-trace
else ifeq ($(TYPE),release)
CARGO_ARGS := --release
TARGET_SUBDIR := release
else
# Unknown TYPE: leave the mapping empty and fail in `build` below. A parse-time
# $(error) here would break TYPE-independent targets (clean, preload, all) and
# fire on any stray TYPE in the environment. Every TYPE consumer (prepare,
# package, deploy, release) runs through `build`, so guarding it covers all.
CARGO_ARGS :=
TARGET_SUBDIR :=
endif

BIN_SRC := src/tuxgt/target/$(TARGET_SUBDIR)/tuxgt

# Baseline CPU for ALL build types (issue #1). Set on the cargo command line
# below so it wins over any RUSTFLAGS in the environment (env beats
# .cargo/config.toml); the config file covers only bare `cargo` invocations.
TUXGT_TARGET_CPU := x86-64

# Same floor for C (build.cpu-baseline): command-line flags, so no
# environment-override games like RUSTFLAGS.
TUXGT_C_BASELINE := -march=x86-64 -mtune=generic

# Deterministic build instant (docs/dev/build.md): version-commit time, used
# for the tarball mtime in every TYPE and exported to cargo for release.
# `?=` lets the environment win (SOURCE_DATE_EPOCH convention); without git
# metadata the wall clock fills in (non-reproducible, still builds).
SOURCE_DATE_EPOCH ?= $(shell git log -1 --format=%ct 2>/dev/null || date +%s)
ifeq ($(TYPE),release)
export SOURCE_DATE_EPOCH
endif

.PHONY: all preload proxy build prepare package deploy release clean clean-full check-baseline

PREPARE := $(BUILD)/pkg/tuxgt

# Live prefix from ~/.config/tuxgt.conf. Missing file, missing/empty TUXGT_DATA, or `/` → error.
CONF_PFX = pfx=$$(grep -E '^TUXGT_DATA=' $(HOME)/.config/tuxgt.conf 2>/dev/null | head -1 | cut -d= -f2- | tr -d '\042\047'); if [ ! -f $(HOME)/.config/tuxgt.conf ] || [ -z "$$pfx" ] || [ "$$pfx" = "/" ]; then echo "error: TUXGT_DATA missing, empty, or '/'; run 'tuxgt install' first"; exit 1; fi

all: preload

preload: $(BUILD)/libtuxgt-launcher.so $(BUILD)/libtuxgt-launcher32.so

LAUNCHER_C := $(wildcard src/launcher/*.c)
$(BUILD)/libtuxgt-launcher.so: $(LAUNCHER_C) src/launcher/tuxgt-launcher.h
	@mkdir -p $(BUILD)
	gcc -shared -fPIC -O2 $(TUXGT_C_BASELINE) -fvisibility=hidden -o $@ $(LAUNCHER_C) -ldl -lcrypto
	@echo "built $@"
$(BUILD)/libtuxgt-launcher32.so: $(LAUNCHER_C) src/launcher/tuxgt-launcher.h
	@mkdir -p $(BUILD)
	@if gcc -m32 -shared -fPIC -O2 $(TUXGT_C_BASELINE) -fvisibility=hidden -o $@ $(LAUNCHER_C) -ldl -lcrypto 2>/dev/null; then echo "built $@"; else echo "skip $@ (no -m32)"; rm -f $@; fi

# Workaround proxy DLL (Windows-only, mingw). Standalone target: no other
# target depends on it. -Wno-cast-function-type: GetProcAddress casts are
# the unavoidable WinAPI idiom; everything else builds -Werror clean.
MINGW_CC := $(firstword \
	$(shell command -v x86_64-w64-mingw32-gcc 2>/dev/null) \
	$(shell command -v x86_64-w64-mingw32-clang 2>/dev/null))

proxy: $(BUILD)/nvngx_dlssnr.dll

PROXY_SRC := src/workarounds/nvngx-dlssnr-proxy
$(BUILD)/nvngx_dlssnr.dll: $(PROXY_SRC)/proxy.c $(PROXY_SRC)/nvngx_dlssnr.def
	@mkdir -p $(BUILD)
	@if [ -z "$(MINGW_CC)" ]; then echo "error: mingw toolchain not found (need x86_64-w64-mingw32-gcc or -clang)" >&2; exit 1; fi
	$(MINGW_CC) -shared -O2 $(TUXGT_C_BASELINE) -s -Wall -Wextra -Werror -Wno-cast-function-type -Wl,--no-insert-timestamp -o $@ $(PROXY_SRC)/proxy.c $(PROXY_SRC)/nvngx_dlssnr.def
	@echo "built $@"

# TYPE=dev (default): fast incremental debug. TYPE=trace: release opts, unstripped +
# line tables. TYPE=release: what ships.
build:
	@case "$(TYPE)" in dev|trace|release) ;; *) echo "error: TYPE must be dev|trace|release (got '$(TYPE)')" >&2; exit 1;; esac
	cd src/tuxgt && RUSTFLAGS="-C target-cpu=$(TUXGT_TARGET_CPU)" cargo build $(CARGO_ARGS) -p tuxgt

# Fail-closed baseline gate (issue #1): the newest tuxgt bin fingerprint for
# this TYPE must carry the exact `-C target-cpu=` pin. Missing fingerprints
# fail (run `build` first). release.sh calls this before tag/push/upload.
check-baseline:
	@case "$(TYPE)" in dev|trace|release) ;; *) echo "error: TYPE must be dev|trace|release (got '$(TYPE)')" >&2; exit 1;; esac
	@newest=$$(ls -t src/tuxgt/target/$(TARGET_SUBDIR)/.fingerprint/tuxgt-*/bin-tuxgt.json 2>/dev/null | head -1); \
	if [ -z "$$newest" ]; then echo "error: no tuxgt fingerprint under target/$(TARGET_SUBDIR) (run 'make build TYPE=$(TYPE)' first)" >&2; exit 1; fi; \
	grep -q '"target-cpu=$(TUXGT_TARGET_CPU)"' "$$newest" || { echo "error: $$newest lacks -C target-cpu=$(TUXGT_TARGET_CPU); refusing to ship" >&2; exit 1; }; \
	echo "baseline verified: $$newest carries target-cpu=$(TUXGT_TARGET_CPU)"

clean:
	@if [ -d "$(BUILD)" ]; then find "$(BUILD)" -mindepth 1 -maxdepth 1 ! -name 'pfx-*' -exec rm -rf {} +; fi
	rm -rf dist
	cd src/tuxgt && cargo clean

clean-full: clean
	rm -rf $(BUILD)/pfx-*

# Product tree under build/pkg/tuxgt. Repo files + our artifacts only.
# No ReShade payloads. Binary comes from src/tuxgt/target/$(TARGET_SUBDIR)/ per TYPE.
prepare: preload build
	rm -rf $(PREPARE)
	mkdir -p $(PREPARE)/bin $(PREPARE)/lib $(PREPARE)/mods/official \
		$(PREPARE)/games $(PREPARE)/downloads $(PREPARE)/config \
		$(PREPARE)/share/applications \
		$(PREPARE)/share/templates \
		$(PREPARE)/share/protonfixes
	install -m755 $(BIN_SRC) $(PREPARE)/bin/tuxgt
	install -m755 src/launcher/tuxgt-launcher $(PREPARE)/bin/tuxgt-launcher
	install -m755 $(BUILD)/libtuxgt-launcher.so $(PREPARE)/lib/libtuxgt-launcher.so
	if [ -f $(BUILD)/libtuxgt-launcher32.so ]; then install -m755 $(BUILD)/libtuxgt-launcher32.so $(PREPARE)/lib/libtuxgt-launcher32.so; fi
	install -m644 src/protonfixes-hook/tuxgt_apply.py src/protonfixes-hook/default.py \
		src/protonfixes-hook/default_wrap.py src/protonfixes-hook/tuxgt.py \
		$(PREPARE)/share/protonfixes/
	for s in 16 24 32 48 64 128 256 512 1024; do \
		install -d $(PREPARE)/share/icons/hicolor/$${s}x$${s}/apps; \
		install -m644 src/tuxgt/tuxgt-app/assets/icons/hicolor/$${s}x$${s}/apps/tuxgt.png $(PREPARE)/share/icons/hicolor/$${s}x$${s}/apps/tuxgt.png; \
	done
	install -m644 mods/official/*.toml $(PREPARE)/mods/official/
	install -m644 mods/templates/*.toml $(PREPARE)/share/templates/
	@printf '%s\n' '[Desktop Entry]' 'Type=Application' 'Name=TuxGT' \
		'Comment=Linux-native manager for injector/runtime mods' 'Exec=tuxgt' 'Icon=tuxgt' \
		'Categories=Game;' 'Terminal=false' \
		> $(PREPARE)/share/applications/tuxgt.desktop
	@echo "prepared $(PREPARE)"

# User tarball: unpacks to a self-contained `tuxgt/` tree. Run
# `tuxgt/bin/tuxgt install` inside it for PATH + desktop links.
package: prepare
	rm -rf dist
	mkdir -p dist
	tar --sort=name --owner=0 --group=0 --mtime=@$(SOURCE_DATE_EPOCH) -cf $(CURDIR)/dist/tuxgt.tar -C $(BUILD)/pkg tuxgt
	gzip -n -f $(CURDIR)/dist/tuxgt.tar
	@echo "packaged dist/tuxgt.tar.gz"

# Developer deploy: tarball the prepared tree and apply it into the live
# PREFIX via the staged binary's `update --from` (the `install.sh`
# mechanism): manifest-tracked owned-file overlay (bin/lib/share +
# mods/official/*.toml; tracked gone ids drop toml + payload dir) plus a
# host refresh as the new binary. Never touches games/, mods/user/,
# registries, or kept official payloads; inside config/ only the
# prefix manifest and host inventory settle. The tag is scraped in
# release.sh's exact `version = "X.Y.Z"` format (empty fails loud).
deploy: prepare
	@$(CONF_PFX); \
	set -e; \
	ver=$$(grep -E '^version = "[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?"$$' src/tuxgt/Cargo.toml | head -1 | cut -d'"' -f2); \
	if [ -z "$$ver" ]; then echo "error: could not parse workspace version from src/tuxgt/Cargo.toml" >&2; exit 1; fi; \
	tarball="$(BUILD)/deploy.tar.gz"; \
	case "$$tarball" in /*) ;; *) tarball="$(CURDIR)/$$tarball";; esac; \
	trap 'rm -f "$$tarball"' EXIT; \
	tar --sort=name --owner=0 --group=0 --mtime=@$(SOURCE_DATE_EPOCH) -czf "$$tarball" -C $(BUILD)/pkg tuxgt; \
	"$(PREPARE)/bin/tuxgt" update --from "$$tarball" --tag "v$$ver" --prefix "$$pfx" --yes; \
	echo "deployed to $$pfx via update --from (v$$ver; games/, mods/user + kept official payloads untouched)"

# Maintainer release: version bump (optional), package, tag, draft GitHub release
# (human publishes). Usage: make release [BUMP=major|minor|patch|beta|promote-beta] [DRY_RUN=1]
# Without BUMP, drafts the current Cargo.toml version unless already out (draft included).
# BUMP=beta drafts a prerelease (minor bump once, then beta counter); promote-beta graduates it.
# release.sh owns the order: preflight -> notes -> BUMP commit -> `make package TYPE=release`
# -> verify the archive carries the bumped version -> tag/push/draft. Packaging after the
# bump is what keeps the tarball's CARGO_PKG_VERSION equal to the version being tagged;
# TYPE=release is forced there because only true release builds ship.
# A dirty tree is refused, not stashed: the version commit must be the tarball's source.
# DRY_RUN=1 prints every step (the package build included) and runs none of them.
release:
	DRY_RUN=$(DRY_RUN) ./.github/release.sh "$(BUMP)"
