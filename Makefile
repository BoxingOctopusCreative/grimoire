# Grimoire production builds (Tauri 2 release bundles)
#
# Usage:
#   make help
#   make build              # current host
#   make build-mac          # macOS (universal by default)
#   make build-windows      # Windows x64 (NSIS when cross-compiling)
#   make build-linux        # Linux x64
#
# Bundles land under src-tauri/target/<triple>/release/bundle/
# (or src-tauri/target/release/bundle/ for a native host build).

PNPM       ?= pnpm
TAURI      ?= $(PNPM) exec tauri
CARGO_XWIN ?= cargo-xwin
RUSTUP     ?= rustup

HOST_OS   := $(shell uname -s 2>/dev/null || echo unknown)
HOST_ARCH := $(shell uname -m 2>/dev/null || echo unknown)

# True on native Windows (cmd/PowerShell make) or MSYS/MinGW shells.
IS_WINDOWS := $(if $(filter Windows_NT,$(OS)),1,$(if $(findstring MINGW,$(HOST_OS)),1,$(if $(findstring MSYS,$(HOST_OS)),1,$(if $(findstring CYGWIN,$(HOST_OS)),1,))))

# Production flags: release mode (tauri build default) and non-interactive CI mode.
TAURI_BUILD_FLAGS ?= --ci

# Optional env file for local Apple signing / notarization (see .env.example).
# Usage: make build-mac   or   make build-mac ENV_FILE=.env.apple
ENV_FILE ?= .env

# Platform triples
MAC_TARGET       ?= universal-apple-darwin
MAC_ARM_TARGET   ?= aarch64-apple-darwin
MAC_INTEL_TARGET ?= x86_64-apple-darwin
WIN_TARGET       ?= x86_64-pc-windows-msvc
LINUX_TARGET     ?= x86_64-unknown-linux-gnu
LINUX_ARM_TARGET ?= aarch64-unknown-linux-gnu

.PHONY: help install build test test-frontend test-rust \
	build-mac build-mac-arm build-mac-intel \
	build-windows build-linux build-linux-arm \
	ensure-frontend ensure-mac-targets ensure-windows-target ensure-linux-target \
	ensure-cargo-xwin clean-dmg-mounts clean

.DEFAULT_GOAL := help

help:
	@echo "Grimoire production builds"
	@echo ""
	@echo "  make build              Build for the current host ($(HOST_OS)/$(HOST_ARCH))"
	@echo "  make build-mac          macOS universal (arm64 + x86_64)"
	@echo "  make build-mac-arm      macOS Apple Silicon only"
	@echo "  make build-mac-intel    macOS Intel only"
	@echo "  make build-windows      Windows x64"
	@echo "  make build-linux        Linux x64"
	@echo "  make build-linux-arm    Linux arm64"
	@echo "  make install            Install frontend deps and common Rust targets"
	@echo "  make test               Run frontend + Rust automated tests"
	@echo "  make test-frontend      Run Vitest suite"
	@echo "  make test-rust          Run cargo test in src-tauri"
	@echo "  make clean              Remove frontend and Rust build outputs"
	@echo "  make clean-dmg-mounts   Unmount leftover Tauri DMG staging volumes (macOS)"
	@echo ""
	@echo "Apple signing (macOS): copy .env.example to .env and fill values, then"
	@echo "  make build-mac"
	@echo "Or: make build-mac ENV_FILE=/path/to/your.env"
	@echo "When APPLE_SIGNING_IDENTITY or APPLE_CERTIFICATE is set, the Gatekeeper"
	@echo "DMG inject is skipped so notarization stays valid."
	@echo ""
	@echo "Run each platform target on that OS when possible. Windows can be"
	@echo "cross-built from macOS/Linux with cargo-xwin + NSIS + LLVM."
	@echo "Linux GUI deps (WebKitGTK, etc.) require a Linux host or matching sysroot."

install: ensure-frontend
	@$(RUSTUP) target add \
		$(MAC_ARM_TARGET) \
		$(MAC_INTEL_TARGET) \
		$(WIN_TARGET) \
		$(LINUX_TARGET) \
		$(LINUX_ARM_TARGET) || true

ensure-frontend:
	$(PNPM) install

# --- tests -------------------------------------------------------------------

test: test-frontend test-rust

test-frontend:
	$(PNPM) test

test-rust:
	cargo test --manifest-path src-tauri/Cargo.toml

ensure-mac-targets:
	@$(RUSTUP) target add $(MAC_ARM_TARGET) $(MAC_INTEL_TARGET)

ensure-windows-target:
	@$(RUSTUP) target add $(WIN_TARGET)

ensure-linux-target:
	@$(RUSTUP) target add $(LINUX_TARGET)

ensure-cargo-xwin:
	@command -v $(CARGO_XWIN) >/dev/null 2>&1 || cargo install cargo-xwin --locked

# Stale create-dmg mounts (/Volumes/dmg.*) break subsequent `tauri build` DMG steps.
clean-dmg-mounts:
ifeq ($(HOST_OS),Darwin)
	@for vol in /Volumes/dmg.* /Volumes/Grimoire*; do \
		if [ -e "$$vol" ]; then \
			echo "Detaching $$vol"; \
			hdiutil detach "$$vol" -force >/dev/null 2>&1 || true; \
		fi; \
	done
	@find src-tauri/target -maxdepth 5 -name 'rw.*.dmg' -delete 2>/dev/null || true
else
	@echo "clean-dmg-mounts is only needed on macOS"
endif

# Load ENV_FILE (if present), run tauri build, then inject Gatekeeper helpers only
# when Apple signing env is absent (injecting after notarize breaks the ticket).
define tauri-build-macos
	set -euo pipefail; \
	if [ -f "$(ENV_FILE)" ]; then \
		echo "Loading $(ENV_FILE)"; \
		set -a; \
		. "./$(ENV_FILE)"; \
		set +a; \
	fi; \
	$(TAURI) build $(TAURI_BUILD_FLAGS) $(1); \
	if [ -n "$${APPLE_SIGNING_IDENTITY:-}" ] || [ -n "$${APPLE_CERTIFICATE:-}" ]; then \
		echo "Skipping Gatekeeper DMG inject (Apple signing env is set)"; \
	else \
		.github/scripts/inject-macos-dmg-installer.sh; \
	fi
endef

# Native production build for the machine you are on.
build: ensure-frontend clean-dmg-mounts
ifeq ($(HOST_OS),Darwin)
	$(call tauri-build-macos,)
else
	$(TAURI) build $(TAURI_BUILD_FLAGS)
endif

# --- macOS -------------------------------------------------------------------

build-mac: ensure-frontend ensure-mac-targets clean-dmg-mounts
ifeq ($(HOST_OS),Darwin)
	$(call tauri-build-macos,--target $(MAC_TARGET))
else
	$(error macOS builds require a Darwin host (got $(HOST_OS)))
endif

build-mac-arm: ensure-frontend clean-dmg-mounts
ifeq ($(HOST_OS),Darwin)
	@$(RUSTUP) target add $(MAC_ARM_TARGET)
	$(call tauri-build-macos,--target $(MAC_ARM_TARGET))
else
	$(error macOS builds require a Darwin host (got $(HOST_OS)))
endif

build-mac-intel: ensure-frontend clean-dmg-mounts
ifeq ($(HOST_OS),Darwin)
	@$(RUSTUP) target add $(MAC_INTEL_TARGET)
	$(call tauri-build-macos,--target $(MAC_INTEL_TARGET))
else
	$(error macOS builds require a Darwin host (got $(HOST_OS)))
endif

# --- Windows -----------------------------------------------------------------

# Native Windows builds produce both the NSIS setup.exe and the WiX MSI.
# Cross-builds from macOS/Linux can only produce NSIS (MSI needs WiX on Windows).
WIN_BUNDLES ?= nsis,msi

build-windows: ensure-frontend ensure-windows-target
ifeq ($(IS_WINDOWS),1)
	$(TAURI) build $(TAURI_BUILD_FLAGS) --target $(WIN_TARGET) --bundles $(WIN_BUNDLES)
else ifeq ($(HOST_OS),Darwin)
	@$(MAKE) ensure-cargo-xwin
	$(TAURI) build $(TAURI_BUILD_FLAGS) --runner $(CARGO_XWIN) --target $(WIN_TARGET) --bundles nsis
else ifeq ($(HOST_OS),Linux)
	@$(MAKE) ensure-cargo-xwin
	$(TAURI) build $(TAURI_BUILD_FLAGS) --runner $(CARGO_XWIN) --target $(WIN_TARGET) --bundles nsis
else
	$(error Unsupported host for Windows builds: $(HOST_OS))
endif

# --- Linux -------------------------------------------------------------------

build-linux: ensure-frontend ensure-linux-target
ifeq ($(HOST_OS),Linux)
	$(TAURI) build $(TAURI_BUILD_FLAGS) --target $(LINUX_TARGET)
else
	$(error Linux builds require a Linux host with WebKitGTK and related deps (got $(HOST_OS)))
endif

build-linux-arm: ensure-frontend
ifeq ($(HOST_OS),Linux)
	@$(RUSTUP) target add $(LINUX_ARM_TARGET)
	$(TAURI) build $(TAURI_BUILD_FLAGS) --target $(LINUX_ARM_TARGET)
else
	$(error Linux builds require a Linux host with WebKitGTK and related deps (got $(HOST_OS)))
endif

# --- cleanup -----------------------------------------------------------------

clean:
	rm -rf build .svelte-kit
	cd src-tauri && cargo clean
