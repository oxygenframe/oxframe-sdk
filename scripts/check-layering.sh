#!/usr/bin/env bash
# Crate layering (ROADMAP task 0): the core and the crates that only move
# frames around (preview, export) must not depend on drawing. Run in CI.
set -euo pipefail
cd "$(dirname "$0")/.."

# Drawing: 2D nodes, layout, text and fonts, both renderers, GPU.
forbidden='^(oxframe-2d|oxframe-text|oxframe-render|vello|vello_[a-z_]+|tiny-skia|taffy|parley|fontique|skrifa|swash|read-fonts|harfrust|wgpu|wgpu-[a-z]+|usvg) '

status=0
for crate in oxframe-core oxframe-preview oxframe-export; do
  found=$(cargo tree -p "$crate" -e normal --all-features --prefix none --target all \
    | grep -E "$forbidden" | sort -u || true)
  if [ -n "$found" ]; then
    echo "error: $crate must not depend on drawing crates, but pulls in:"
    echo "$found" | sed 's/^/  /'
    status=1
  else
    echo "ok: $crate has no drawing dependencies"
  fi
done

# 3D (ROADMAP task 16) sits beside 2D: oxframe-3d needs only the core, and
# the 2D crates never pull 3D in.
check() {
  local crate=$1 pattern=$2 what=$3
  local found
  found=$(cargo tree -p "$crate" -e normal --all-features --prefix none --target all \
    | grep -E "$pattern" | sort -u || true)
  if [ -n "$found" ]; then
    echo "error: $crate must not depend on $what, but pulls in:"
    echo "$found" | sed 's/^/  /'
    status=1
  else
    echo "ok: $crate has no $what dependencies"
  fi
}
check oxframe-3d "$forbidden" "drawing (2D)"
for crate in oxframe-2d oxframe-render oxframe-components; do
  check "$crate" '^oxframe-3d ' "3D"
done
exit $status
