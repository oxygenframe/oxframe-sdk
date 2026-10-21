#!/usr/bin/env bash
# Everything a release can break, run locally against this checkout before
# tagging vX.Y.Z (see AGENTS.md, "Releasing"). Expects the sibling checkouts
# ../oxframe-cli and ../oxframe-web.
#
# CI only checks each repository against the published SDK; an API change
# here can still break the CLI's project template, the site's examples and
# docs, and the compile service's sandbox template. This catches that.
# The last step renders this version's changelog as an MP4, the same way
# release-video-action does, using the CLI and this SDK checkout.
# Before that, the current site docs (not an archived snapshot) must name
# every public API this version's changelog puts in backticks, and the
# Going further pages that teach by picture must include a live player.
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

step "Site claims match the recorded measurements"
(cd "$web/apps/site" && bun scripts/check-claims.ts)

step "Docs describe this version's features"
python3 "$sdk/scripts/check-docs.py"

step "Site and compile service on this SDK (examples, docs, sandbox template, e2e)"
(cd "$web" && OXFRAME_SDK_PATH="$sdk" bun run verify)

step "Container images on this SDK (podman or docker): build, run offline, smoke test"
(cd "$web" && OXFRAME_SDK_PATH="$sdk" ./scripts/image-check.sh)

step "Release video from this version's changelog"
action=$sdk/../release-video-action
[[ -f $action/main.mjs ]] || {
  printf 'release-check: expected the release-video-action checkout at %s\n' "$action" >&2
  exit 1
}
command -v node >/dev/null || { printf 'release-check: node is required to build the release video\n' >&2; exit 1; }
command -v ffmpeg >/dev/null || { printf 'release-check: ffmpeg is required to build the release video\n' >&2; exit 1; }
(cd "$cli" && cargo build --bin oxframe)
target_dir=$(cd "$cli" && cargo metadata --format-version 1 --no-deps --offline |
  python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
bin=$target_dir/debug/oxframe
[[ -x $bin ]] || { printf 'release-check: oxframe binary not found at %s\n' "$bin" >&2; exit 1; }
version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -1)
notes=$(awk -v v="$version" '$0 ~ "^## " v "( |$)" {on=1; next} /^## / {on=0} on' CHANGELOG.md)
[[ -n $notes ]] || { printf 'release-check: CHANGELOG.md has no section for %s\n' "$version" >&2; exit 1; }
outdir=$sdk/target/release-check
rm -rf "$outdir"
mkdir -p "$outdir"
(
  cd "$outdir"
  # A token would upload onto the existing GitHub release. This step only writes the file.
  env -u GITHUB_TOKEN -u GH_TOKEN \
    RELEASE_BODY="$notes" \
    RELEASE_TAG="v$version" \
    RELEASE_REPO="oxygenframe/oxframe-sdk" \
    OXFRAME_BIN="$bin" \
    OXFRAME_SDK_PATH="$sdk" \
    node "$action/main.mjs"
)
mp4=$outdir/out/release.mp4
[[ -s $mp4 ]] || { printf 'release-check: oxframe render did not write %s\n' "$mp4" >&2; exit 1; }
cp "$mp4" "$outdir/release.mp4"
duration=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$outdir/release.mp4")
printf 'release video: %s (%ss)\n' "$outdir/release.mp4" "$duration"

printf '\nrelease-check: all green\n'
