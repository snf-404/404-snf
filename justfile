# SPDX-License-Identifier: Apache-2.0
#
# Convenience tasks for 404-snf. `just` (https://github.com/casey/just) is the
# runner; `csti` is the consortium build CLI (from ../consortium/crates/
# consortium-cli); `vp` provides repository-level JavaScript tooling while the
# Lingguang app keeps its official npm-managed toolchain.

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

# ── JavaScript tooling + Lingguang app ───────────────────────────────────────

# Install repository-level Vite+ tooling.
js-install:
    pnpm install

# Run repository-level Vite+ checks.
js-check:
    vp check

# Install the Lingguang app from its official npm lockfile.
lingguang-install:
    vp run -w install:lingguang

# Run the Lingguang scaffold's complete quality gate.
lingguang-check:
    vp run -w check:lingguang

# Build the Lingguang flash app.
lingguang-build:
    vp run -w build:lingguang

# Run the Lingguang development server.
lingguang-dev:
    vp run -w dev:lingguang

# ── Linux dev container (Apple `container` on macOS) ──────────────────────────
# The Rust host checks above run natively on macOS (radar/ble host libs). But the
# CA35 app links against Linux (`bluer`, UIO, `sysroot = "/"`) and the endpoints
# are built by `csti`, so anything beyond `cargo check` runs inside a native
# arm64 Linux VM via Apple `container`. See Dockerfile.linux.
#
# 404-snf depends on consortium by path (`../consortium`), so the container
# mounts BOTH repos under /work: this repo at /work/404-snf, consortium at
# /work/consortium — keeping `../consortium` resolvable inside the VM.
#
# `--dns` is not optional behind a VPN that hands out fake-ip DNS (198.18.0.0/15):
# the VM inherits the unroutable address and every network call hangs silently.

snf_image        := "snf-linux"
snf_dns          := env_var_or_default("SNF_LINUX_DNS", "1.1.1.1")
snf_cpus         := env_var_or_default("SNF_LINUX_CPUS", "6")
snf_memory       := env_var_or_default("SNF_LINUX_MEMORY", "8192M")
consortium_dir   := parent_directory(justfile_directory()) / "consortium"
snf_ctx          := justfile_directory() / "target/container-context"

# Start Apple `container` services + builder with a routable resolver, and create
# the named volumes that keep Linux build artifacts out of the macOS ./target.
container-setup:
    container system start
    @container builder status >/dev/null 2>&1 \
      && echo "builder already running (restart with 'container builder stop' if DNS changed)" \
      || container builder start --dns {{ snf_dns }} --cpus 4 --memory 4096M
    @container volume create snf-target >/dev/null 2>&1 || true
    @container volume create snf-cargo  >/dev/null 2>&1 || true

# Build the Linux image (Rust toolchains + all crate build deps).
container-image: container-setup
    @mkdir -p {{ snf_ctx }}
    container build --target base -t {{ snf_image }}:base \
      -f {{ justfile_directory() }}/Dockerfile.linux {{ snf_ctx }}

# Run a command in the image, e.g. `just snf cargo check --workspace`.
snf +cmd: (container-run "base" cmd)

# Shared runner. Mounts both repos + the target/cargo volumes. The target volume
# shadows the repo's own ./target so the excluded endpoint crates still build into
# their per-crate target/ dirs where the `csti` pipeline expects their ELFs.
[private]
container-run tag +cmd:
    container run --rm --dns {{ snf_dns }} \
      --cpus {{ snf_cpus }} --memory {{ snf_memory }} \
      -v {{ justfile_directory() }}:/work/404-snf \
      -v {{ consortium_dir }}:/work/consortium \
      -v snf-target:/work/404-snf/target \
      -v snf-cargo:/cargo-registry \
      -e CARGO_HOME=/cargo-registry \
      -w /work/404-snf {{ snf_image }}:{{ tag }} \
      bash -lc '{{ cmd }}'

# Drop into an interactive shell in the image.
container-shell:
    container run --rm -it --dns {{ snf_dns }} \
      --cpus {{ snf_cpus }} --memory {{ snf_memory }} \
      -v {{ justfile_directory() }}:/work/404-snf \
      -v {{ consortium_dir }}:/work/consortium \
      -v snf-target:/work/404-snf/target \
      -v snf-cargo:/cargo-registry \
      -e CARGO_HOME=/cargo-registry \
      -w /work/404-snf {{ snf_image }}:base bash

# Install `csti` (the consortium build CLI) into the cargo volume, once per volume.
# It lands in /cargo-registry/bin, which the image puts on PATH.
container-csti-install:
    just snf 'cargo install --path /work/consortium/crates/consortium-cli --root /cargo-registry'

# Build the endpoints (app + mcu) inside the container via `csti`.
container-build:
    just snf 'csti build --manifest Consortium.toml'

# Remove the cached Linux build volumes and context.
container-clean:
    -container volume delete snf-target
    -container volume delete snf-cargo
    -rm -rf {{ snf_ctx }}
