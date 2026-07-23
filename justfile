# SPDX-License-Identifier: Apache-2.0
#
# Convenience tasks for 404-snf. `just` (https://github.com/casey/just) is the
# runner; `csti` is the consortium build CLI (from ../consortium/crates/
# consortium-cli); `vp` is Vite+ for the JavaScript workspace.

# List available recipes.
default:
    @just --list

# ── Rust: host-checkable library crates ──────────────────────────────────────

# Type-check the workspace libraries (shared, radar, ble, fatigue). The radar
# `cxx` bridge builds its Rust-only stub unless MMWAVE_SDK_PATH is set.
check:
    cargo check --workspace

# Format + clippy the workspace libraries.
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets

test:
    cargo test --workspace

# ── Rust: endpoints (built by the consortium pipeline, not plain cargo) ───────

# Generate `consortium.gen.rs` for each endpoint and cross-compile app + mcu.
# Requires `csti` on PATH plus the aarch64-unknown-linux-gnu and
# thumbv8m.main-none-eabihf targets.
build:
    csti build --manifest Consortium.toml

# ── JavaScript workspace (Vite+) ─────────────────────────────────────────────

# Install JS dependencies across packages/* and apps/*.
js-install:
    vp install

# Build the shared TS libraries under packages/*.
js-pack:
    vp pack

# Run the frontend dev server (apps/www is provisioned externally; see its README).
js-dev:
    vp dev
