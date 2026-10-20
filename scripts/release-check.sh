#!/usr/bin/env bash
# Everything a release can break, run locally against this checkout before
# tagging vX.Y.Z (see AGENTS.md, "Releasing"). Expects the sibling checkouts
# ../oxframe-cli and ../oxframe-web.
#
# CI only checks each repository against the published SDK; an API change
# here can still break the CLI's project template, the site's examples and
# docs, and the compile service's sandbox template. This catches that.
set -euo pipefail
cd "$(dirname "$0")/.."
sdk=$PWD
cli=$sdk/../oxframe-cli
web=$sdk/../oxframe-web
step() { printf '\n== %s\n' "$*"; }

step "SDK: format, layering, clippy, tests, docs, wasm"
cargo fmt --all --check
./scripts/check-layering.sh
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
OXFRAME_SEEK_BUDGET_MS=400 cargo test --release -p oxframe-examples --test acceptance
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo build --target wasm32-unknown-unknown -p oxframe-core -p oxframe-2d -p oxframe-render -p oxframe-protocol -p oxframe
cargo clippy --target wasm32-unknown-unknown -p oxframe --features wasm -- -D warnings

step "CLI: tests, and a new project built and previewed on this SDK"
(cd "$cli" && cargo test && OXFRAME_SDK_PATH="$sdk" cargo test --test e2e -- --ignored)

step "Site and compile service on this SDK (examples, docs, sandbox template, e2e)"
(cd "$web" && OXFRAME_SDK_PATH="$sdk" bun run verify)

printf '\nrelease-check: all green\n'
