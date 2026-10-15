# Grimoire release builds.
#
# Usage:
#   make help
#   make build              # release binary for this machine
#   make build-mac          # macOS .app and DMG (loads .env for signing)
#   make build-windows      # Windows x64, on Windows
#   make build-linux        # Linux x64 tarball, deb, and rpm, on Linux

CARGO ?= cargo

HOST_OS   := $(shell uname -s 2>/dev/null || echo unknown)
HOST_ARCH := $(shell uname -m 2>/dev/null || echo unknown)

IS_WINDOWS := $(if $(filter Windows_NT,$(OS)),1,$(if $(findstring MINGW,$(HOST_OS)),1,$(if $(findstring MSYS,$(HOST_OS)),1,$(if $(findstring CYGWIN,$(HOST_OS)),1,))))

# Optional env file for local Apple signing / notarization (see .env.example).
ENV_FILE ?= .env

MAC_ARM_TARGET   ?= aarch64-apple-darwin
MAC_INTEL_TARGET ?= x86_64-apple-darwin
WIN_TARGET       ?= x86_64-pc-windows-msvc
LINUX_TARGET     ?= x86_64-unknown-linux-gnu

ifeq ($(HOST_ARCH),x86_64)
MAC_ARCH_LABEL := macos_x64
else
MAC_ARCH_LABEL := macos_arm64
endif

.PHONY: help build test test-rust \
	build-mac build-mac-arm build-mac-intel \
	build-windows build-linux \
	clean

.DEFAULT_GOAL := help

help:
	@echo "Grimoire release builds"
	@echo ""
	@echo "  make build              Release binary for this machine ($(HOST_OS)/$(HOST_ARCH))"
	@echo "  make build-mac          macOS disk image for this machine"
	@echo "  make build-mac-arm      macOS Apple Silicon disk image"
	@echo "  make build-mac-intel    macOS Intel disk image"
	@echo "  make build-windows      Windows x64 executable"
	@echo "  make build-linux        Linux x64 tarball, deb, and rpm"
	@echo "  make test               Run cargo test"
	@echo "  make clean              Remove Rust build outputs and dist/"
	@echo ""
	@echo "Apple signing (macOS): copy .env.example to .env and fill values, then"
	@echo "  make build-mac"
	@echo "Or: make build-mac ENV_FILE=/path/to/your.env"
	@echo "CI reads the signing identity from APPLE_CERTIFICATE. For local Keychain"
	@echo "signing, set APPLE_SIGNING_IDENTITY in .env."
	@echo ""
	@echo "Linux builds need Wayland, xkbcommon, and fontconfig."
	@echo "Linux packages also need binutils, dpkg-dev, and rpm"
	@echo "  (Ubuntu: sudo apt-get install binutils dpkg-dev rpm desktop-file-utils)."

test: test-rust

test-rust:
	$(CARGO) test --manifest-path app/Cargo.toml

build:
	$(CARGO) build --manifest-path app/Cargo.toml --release --bin Grimoire

build-mac:
ifeq ($(HOST_OS),Darwin)
	$(CARGO) build --manifest-path app/Cargo.toml --release --bin Grimoire
	$(call sign-and-package,app/target/release/Grimoire,$(MAC_ARCH_LABEL))
else
	$(error macOS builds require a Darwin host (got $(HOST_OS)))
endif

build-mac-arm:
ifeq ($(HOST_OS),Darwin)
	rustup target add $(MAC_ARM_TARGET)
	$(CARGO) build --manifest-path app/Cargo.toml --release --bin Grimoire --target $(MAC_ARM_TARGET)
	$(call sign-and-package,app/target/$(MAC_ARM_TARGET)/release/Grimoire,macos_arm64)
else
	$(error macOS builds require a Darwin host (got $(HOST_OS)))
endif

build-mac-intel:
ifeq ($(HOST_OS),Darwin)
	rustup target add $(MAC_INTEL_TARGET)
	$(CARGO) build --manifest-path app/Cargo.toml --release --bin Grimoire --target $(MAC_INTEL_TARGET)
	$(call sign-and-package,app/target/$(MAC_INTEL_TARGET)/release/Grimoire,macos_x64)
else
	$(error macOS builds require a Darwin host (got $(HOST_OS)))
endif

build-windows:
ifeq ($(IS_WINDOWS),1)
	$(CARGO) build --manifest-path app/Cargo.toml --release --bin Grimoire --target $(WIN_TARGET)
else
	$(error Windows builds require a Windows host (got $(HOST_OS)))
endif

build-linux:
ifeq ($(HOST_OS),Linux)
	rustup target add $(LINUX_TARGET)
	$(CARGO) build --manifest-path app/Cargo.toml --release --bin Grimoire --target $(LINUX_TARGET)
	chmod +x .github/scripts/package-linux.sh
	.github/scripts/package-linux.sh \
		app/target/$(LINUX_TARGET)/release/Grimoire \
		dist \
		linux_x64
else
	$(error Linux builds require a Linux host (got $(HOST_OS)))
endif

clean:
	cd app && $(CARGO) clean
	rm -rf dist

define sign-and-package
	set -euo pipefail; \
	if [ -f "$(ENV_FILE)" ]; then \
		echo "Loading $(ENV_FILE)"; \
		set -a; \
		. "./$(ENV_FILE)"; \
		set +a; \
	fi; \
	chmod +x .github/scripts/package-macos.sh; \
	.github/scripts/package-macos.sh "$(1)" dist "$(2)"
endef
