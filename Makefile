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

.PHONY: all preload build prepare package deploy release clean clean-full

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

# TYPE=dev (default): fast incremental debug. TYPE=trace: release opts, unstripped +
# line tables. TYPE=release: what ships.
build:
	@case "$(TYPE)" in dev|trace|release) ;; *) echo "error: TYPE must be dev|trace|release (got '$(TYPE)')" >&2; exit 1;; esac
	cd src/tuxgt && cargo build $(CARGO_ARGS) -p tuxgt

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

# Maintainer release: version bump (optional), tag, draft GitHub release (human publishes).
# Usage: make release [BUMP=major|minor|patch|beta|promote-beta] [DRY_RUN=1]
# Without BUMP, drafts the current Cargo.toml version unless already out (draft included).
# BUMP=beta drafts a prerelease (minor bump once, then beta counter); promote-beta graduates it.
# Always builds TYPE=release: only true release builds ship.
# A dirty tree is stashed before the build (tracked changes, then untracked files
# surfaced by HEAD ignore rules) and popped after, so a no-BUMP tarball matches
# its tag's HEAD. With BUMP the version commit lands after the build, so the
# tarball carries the pre-bump version (TASKS: release-bump-tarball-stale).
# DRY_RUN=1 only echoes release.sh's mutating steps; the build and the
# stash/pop cycle still run for real, the pop restoring the tree afterwards.
# NOTE: never 'make -n release' to preview: the parent shell (both stash pushes
# included) executes for real under -n; only the nested package build is skipped
# via propagated -n.
release:
	@set -e; \
	if git stash list | grep -q tuxgt-release-autostash; then \
	  echo "error: leftover tuxgt-release-autostash entr(ies); 'git stash pop'/drop to recover first" >&2; exit 1; \
	fi; \
	t=0; u=0; \
	trap 's=$$?; if [ "$$u" = 1 ]; then git stash pop -q || exit 1; fi; if [ "$$t" = 1 ]; then git stash pop -q || s=1; fi; exit $$s' EXIT; \
	if ! git diff --quiet || ! git diff --cached --quiet; then \
	  echo "release: stashing tracked changes:"; git status --porcelain; \
	  git stash push -q -m tuxgt-release-autostash; t=1; \
	fi; \
	if [ -n "$$(git status --porcelain)" ]; then \
	  echo "release: stashing surfaced untracked files:"; git status --porcelain; \
	  git stash push -q -u -m tuxgt-release-autostash-untracked; u=1; \
	fi; \
	$(MAKE) package TYPE=release; \
	DRY_RUN=$(DRY_RUN) ./.github/release.sh "$(BUMP)"
