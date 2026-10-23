#!/usr/bin/env bash
# Renders the `quickstart` example and turns it into docs/quickstart.gif,
# the animation at the top of the README's quickstart. Needs ffmpeg.
set -euo pipefail
cd "$(dirname "$0")/.."
out=target/readme-gif
mkdir -p "$out" docs
cargo run -q --release -p oxframe-examples --bin quickstart -- render --out "$out/quickstart.mp4"
# Two passes: a palette from the whole clip, then the GIF with it.
filters="fps=15,scale=800:-1:flags=lanczos"
ffmpeg -loglevel error -y -i "$out/quickstart.mp4" -vf "$filters,palettegen=stats_mode=diff" "$out/palette.png"
ffmpeg -loglevel error -y -i "$out/quickstart.mp4" -i "$out/palette.png" \
  -lavfi "$filters [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" \
  docs/quickstart.gif
ls -l docs/quickstart.gif
