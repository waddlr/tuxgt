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

.PHONY: all preload proxy build prepare package deploy release clean clean-full check-baseline

PREPARE := $(BUILD)/pkg/tuxgt

# Live prefix from ~/.config/tuxgt.conf. Missing file, missing/empty TUXGT_DATA, or `/` → error.
CONF_PFX = pfx=$$(grep -E '^TUXGT_DATA=' $(HOME)/.config/tuxgt.conf 2>/dev/null | head -1 | cut -d= -f2- | tr -d '\042\047'); if [ ! -f $(HOME)/.config/tuxgt.conf ] || [ -z "$$pfx" ] || [ "$$pfx" = "/" ]; then echo "error: TUXGT_DATA missing, empty, or '/'; run 'tuxgt install' first"; exit 1; fi

all: preload

preload: $(BUILD)/libtuxgt-launcher.so $(BUILD)/libtuxgt-launcher32.so

LAUNCHER_C := $(wildcard src/launcher/*.c)
$(BUILD)/libtuxgt-launcher.so: $(LAUNCHER_C) src/launcher/tuxgt-launcher.h
	@mkdir -p $(BUILD)
	gcc -shared -fPIC -O2 -fvisibility=hidden -o $@ $(LAUNCHER_C) -ldl -lcrypto
	@echo "built $@"
$(BUILD)/libtuxgt-launcher32.so: $(LAUNCHER_C) src/launcher/tuxgt-launcher.h
	@mkdir -p $(BUILD)
	@if gcc -m32 -shared -fPIC -O2 -fvisibility=hidden -o $@ $(LAUNCHER_C) -ldl -lcrypto 2>/dev/null; then echo "built $@"; else echo "skip $@ (no -m32)"; rm -f $@; fi

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
	$(MINGW_CC) -shared -O2 -s -Wall -Wextra -Werror -Wno-cast-function-type -o $@ $(PROXY_SRC)/proxy.c $(PROXY_SRC)/nvngx_dlssnr.def
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
	tar -czf $(CURDIR)/dist/tuxgt.tar.gz -C $(BUILD)/pkg tuxgt
	@echo "packaged dist/tuxgt.tar.gz"

# Developer overlay: refresh bin/lib/share from the prepared tree into the
# live PREFIX. Official recipe TOMLs are synced (write/update; gone ids drop
# the toml and that id's payload dir). Never touches games/, downloads/,
# config/, mods/user/, registries, or payload dirs of still-shipped officials.
deploy: prepare
	@$(CONF_PFX); \
	install -d "$$pfx/bin" "$$pfx/lib" "$$pfx/share/applications" \
		"$$pfx/share/templates" "$$pfx/share/protonfixes" "$$pfx/mods/official"; \
	install -m755 $(PREPARE)/bin/tuxgt "$$pfx/bin/tuxgt"; \
	install -m755 $(PREPARE)/bin/tuxgt-launcher "$$pfx/bin/tuxgt-launcher"; \
	install -m755 $(PREPARE)/lib/libtuxgt-launcher.so "$$pfx/lib/libtuxgt-launcher.so"; \
	if [ -f "$(PREPARE)/lib/libtuxgt-launcher32.so" ]; then install -m755 "$(PREPARE)/lib/libtuxgt-launcher32.so" "$$pfx/lib/libtuxgt-launcher32.so"; fi; \
	install -m644 $(PREPARE)/share/protonfixes/* "$$pfx/share/protonfixes/"; \
	for s in 16 24 32 48 64 128 256 512 1024; do \
		install -d "$$pfx/share/icons/hicolor/$${s}x$${s}/apps"; \
		install -m644 $(PREPARE)/share/icons/hicolor/$${s}x$${s}/apps/tuxgt.png "$$pfx/share/icons/hicolor/$${s}x$${s}/apps/tuxgt.png"; \
	done; \
	install -m644 $(PREPARE)/mods/official/*.toml "$$pfx/mods/official/"; \
	for f in "$$pfx/mods/official"/*.toml; do \
		[ -f "$$f" ] || continue; \
		b=$$(basename "$$f"); \
		if [ ! -f "$(PREPARE)/mods/official/$$b" ]; then \
			id=$${b%.toml}; \
			rm -f "$$f"; \
			rm -rf "$$pfx/mods/official/$$id"; \
		fi; \
	done; \
	install -m644 $(PREPARE)/share/templates/*.toml "$$pfx/share/templates/"; \
	if [ ! -f "$$pfx/share/applications/tuxgt.desktop" ]; then \
		install -m644 $(PREPARE)/share/applications/tuxgt.desktop \
			"$$pfx/share/applications/tuxgt.desktop"; \
	fi; \
	echo "deployed to $$pfx (games/, downloads/, config/, mods/user + kept official payloads untouched)"

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
