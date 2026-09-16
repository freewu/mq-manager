# ─────────────────────────────────────────────────────────────────────────────
# MQ Manager — a single entry point for setup, development, tests and packaging.
#
#   just            list every recipe
#   just bootstrap  fresh clone: check the toolchain, then install dependencies
#   just dev        run the desktop app
#
# Kafka is only the first driver: the recipes stay broker agnostic so the same
# workflow covers RabbitMQ / RocketMQ / MQTT builds too.
# ─────────────────────────────────────────────────────────────────────────────

# Backend log verbosity for every `cargo` driven recipe.
# Override per run:  RUST_LOG=trace just dev
export RUST_LOG := env_var_or_default('RUST_LOG', 'mq_manager_lib=debug,rdkafka=warn')

# First recipe: `just` on its own prints the menu.
default:
    @just --list

# ── setup ────────────────────────────────────────────────────────────────────

# Check the toolchain and wire up the native build environment (CMake, MSVC).
setup:
    node scripts/setup.mjs

# Report-only toolchain check — never writes a file.
doctor:
    node scripts/setup.mjs --check

# Install frontend dependencies with pnpm.
install:
    pnpm install

# Everything a fresh clone needs, in order.
bootstrap: setup install

# ── develop ──────────────────────────────────────────────────────────────────

# Run the desktop app (starts Vite, compiles the Rust backend, opens a window).
dev:
    pnpm tauri dev

# Vite only, in a browser: useful for pure CSS/UI work, the IPC bridge is absent.
web:
    pnpm dev

# Format the frontend (prettier) and the backend (rustfmt).
fmt:
    pnpm format
    cargo fmt --manifest-path src-tauri/Cargo.toml

# Fail if anything is unformatted. Used by CI.
fmt-check:
    pnpm format:check
    cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

# TypeScript type check (tsc --noEmit).
typecheck:
    pnpm typecheck

# Fast Rust type check, no codegen for the linker.
rust-check:
    cargo check --manifest-path src-tauri/Cargo.toml --message-format short

# Everything CI runs.
check: typecheck rust-check
    pnpm test
    cargo test --manifest-path src-tauri/Cargo.toml

alias lint := check

# Frontend unit tests (vitest).
test:
    pnpm test

# Frontend unit tests in watch mode.
test-watch:
    pnpm test:watch

# Backend unit tests.
rust-test:
    cargo test --manifest-path src-tauri/Cargo.toml

# ── assets ───────────────────────────────────────────────────────────────────

# Regenerate every icon from code (PNG, ICO, ICNS) — no binaries are committed.
icons:
    node scripts/gen-icon.mjs

# ── build & package ──────────────────────────────────────────────────────────

# Production build of the web view only, into `dist/`.
build:
    pnpm build

# Build the desktop app and its installers.
package:
    pnpm tauri build

alias bundle := package

# Package for explicit bundle targets, e.g. `just package-for nsis`.
package-for *BUNDLES:
    pnpm tauri build --bundles {{BUNDLES}}

# Windows installer (NSIS, per-user install, English + Simplified Chinese).
package-windows:
    pnpm tauri build --bundles nsis

# macOS app + dmg.
package-macos:
    pnpm tauri build --bundles app,dmg

# Linux deb + AppImage.
package-linux:
    pnpm tauri build --bundles deb,appimage

# Debug build with devtools and symbols — much faster, no optimization.
package-debug:
    pnpm tauri build --debug

# ── local broker fixtures ────────────────────────────────────────────────────

# Start every fixture (Kafka + UI, RabbitMQ, RocketMQ).
all-up:
    docker compose up -d

# Start a single node Kafka (KRaft, no ZooKeeper) plus a web UI.
kafka-up:
    docker compose up -d kafka kafka-ui

# Stop the fixture, keeping its data volume.
kafka-down:
    docker compose down

# Stop the fixture and delete its data.
kafka-reset:
    docker compose down -v

# Follow the broker log.
kafka-logs:
    docker compose logs -f kafka

# Create sample topics and push a few messages through them.
kafka-seed:
    node scripts/kafka-seed.mjs

# Start RabbitMQ with the management plugin (AMQP 5672, management 15672).
rabbit-up:
    docker compose up -d rabbitmq

# Stop the RabbitMQ fixture, keeping its data volume.
rabbit-down:
    docker compose stop rabbitmq

# Stop the RabbitMQ fixture and delete its data.
rabbit-reset:
    docker compose rm -sfv rabbitmq
    docker volume rm -f mq-manager_rabbitmq-data

# Follow the RabbitMQ log.
rabbit-logs:
    docker compose logs -f rabbitmq

# Start a RocketMQ nameserver and one broker (nameserver 9876, broker 10911).
rocket-up:
    docker compose up -d rocketmq-namesrv rocketmq-broker

# Stop the RocketMQ fixture, keeping its store.
rocket-down:
    docker compose stop rocketmq-broker rocketmq-namesrv

# Stop the RocketMQ fixture and delete its store.
rocket-reset:
    docker compose rm -sfv rocketmq-broker rocketmq-namesrv
    docker volume rm -f mq-manager_rocketmq-broker-store mq-manager_rocketmq-namesrv-logs

# Follow the RocketMQ broker log.
rocket-logs:
    docker compose logs -f rocketmq-broker

# ── release ──────────────────────────────────────────────────────────────────

# Print the version the app currently carries.
app-version:
    node scripts/version.mjs

# Bump the version in every file that carries it — pushing that change is what
# publishes a release (see AGENTS.md).
set-version VERSION:
    node scripts/version.mjs {{VERSION}}

# Preview whether this commit would release, and the release message.
release-plan:
    node scripts/release-plan.mjs

# Cut a release. Rename the CHANGELOG's `[Unreleased]` section to `[{{VERSION}}]`
# first: the release notes are taken from it.
release VERSION:
    node scripts/version.mjs {{VERSION}}
    just check
    git add -A
    git commit -m "chore(release): v{{VERSION}}"
    git push origin main

# ── housekeeping ─────────────────────────────────────────────────────────────

# Remove Rust build artefacts and the frontend bundle.
clean:
    cargo clean --manifest-path src-tauri/Cargo.toml
    node -e "fs.rmSync('dist',{recursive:true,force:true})"

# Report the tool versions this checkout is built with.
version:
    node -v
    pnpm -v
    cargo -V
    rustc -V
    just --version
