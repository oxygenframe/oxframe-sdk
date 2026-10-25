#!/usr/bin/env bash
# Every earlier release of `oxframe` in this minor series must still build
# against this checkout's other crates. Projects pin `oxframe = "=0.23.4"`
# (oxframe init does), but its own dependencies (oxframe-export, …) resolve
# to the newest 0.23.x, so a new public struct field or a removed item in a
# patch release breaks them (0.23.5 added a field to `MixRequest` and broke
# every project on 0.23.0–0.23.4).
#
#   ./scripts/check-semver.sh            # every earlier 0.X.y tag
#   ./scripts/check-semver.sh 0.23.4     # one release
set -euo pipefail
cd "$(dirname "$0")/.."
sdk=$PWD
version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -1)
minor=${version%.*}
# Yanked on crates.io: no project resolves to them any more.
# 0.23.5: its `oxframe` set the field `MixRequest::loudness`, gone in 0.23.6.
yanked=" 0.23.5 "
if [[ $# -gt 0 ]]; then
  releases=("$@")
else
  mapfile -t releases < <(git tag -l "v$minor.*" | sed 's/^v//' | sort -V |
    awk -v v="$version" -v y="$yanked" '$0 != v && index(y, " " $0 " ") == 0')
fi
if [[ ${#releases[@]} -eq 0 ]]; then
  echo "check-semver: no earlier $minor.x release; nothing to check"
  exit 0
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/src"
echo 'fn main() {}' > "$work/src/main.rs"
# One target folder for every probe, so dependencies build once.
export CARGO_TARGET_DIR=$sdk/target/semver-probe
failed=()
for prev in "${releases[@]}"; do
  {
    cat <<TOML
[package]
name = "semver-probe"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
oxframe = { version = "=$prev", features = ["video", "icons", "3d", "math", "unstable-node-api"] }

[patch.crates-io]
TOML
    for dir in crates/*/; do
      name=$(basename "$dir")
      [[ $name == oxframe ]] && continue
      echo "$name = { path = \"$sdk/$dir\" }"
    done
  } > "$work/Cargo.toml"
  rm -f "$work/Cargo.lock"
  if (cd "$work" && cargo check -q 2>&1); then
    echo "check-semver: oxframe $prev builds with the crates of $version"
  else
    failed+=("$prev")
  fi
done
if [[ ${#failed[@]} -gt 0 ]]; then
  echo "check-semver: oxframe ${failed[*]} no longer build with the crates of $version." >&2
  echo "A patch release must not add a public struct field, remove an item or change a signature." >&2
  exit 1
fi
