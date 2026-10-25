#!/usr/bin/env bash
# The numbers on oxframe.dev (oxframe-web/apps/site/src/lib/measurements.json),
# measured again on this machine with this checkout of the SDK and the CLI
# next to it. Prints the JSON; copy the fields you trust into that file.
#
#   ./scripts/measure.sh > /tmp/measurements.json
#   RUNS=5 ./scripts/measure.sh     # each timing five times: median, min, max
#
# Runs on Linux and macOS (CI: .github/workflows/measure.yml on several
# machines at every release).
#
# The scene is the one the site describes: a title card, a bullet list and
# an end card, 30 seconds long. Needs ffmpeg for the render.
set -euo pipefail
cd "$(dirname "$0")/.."
sdk=$PWD
cli=$sdk/../oxframe-cli
version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -1)

(cd "$cli" && cargo build -q --release --bin oxframe)
oxframe=$(cd "$cli" && cargo metadata --format-version 1 --no-deps --offline |
  python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/release/oxframe

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
"$oxframe" init talk --sdk-path "$sdk" >/dev/null
cd talk
rm -f src/*.meta.json
cat > src/main.rs <<'EOF'
use oxframe::prelude::*;

#[oxframe::scene]
async fn talk(view: View) {
    let title = view.add(TitleCard::new("Video as code").subtitle("Measured"));
    title.enter().await;
    title.next().await;
    wait_for(6.0).await;
    title.exit().await;
    let list = view.add(BulletList::new(["Scenes are async Rust", "Every value is a signal", "The agent checks its frames"]));
    list.enter().await;
    for _ in 0..3 {
        list.next().await;
        wait_for(4.0).await;
    }
    list.exit().await;
    let end = view.add(EndCard::new("oxframe.dev").note("made with oxframe"));
    end.enter().await;
    end.next().await;
    wait_for(5.0).await;
}

fn main() {
    Project::new("talk").scene(talk()).main();
}
EOF

runs=${RUNS:-1}
seconds() { python3 -c 'import sys,time; print(f"{time.time():.3f}")'; }
elapsed() { python3 -c "print(f'{$2 - $1:.2f}')"; }
# `timed <command…>`: wall time of each of $runs runs, space-separated.
timed() {
  local all=() t0 t1
  for _ in $(seq "$runs"); do
    t0=$(seconds); "$@" >/dev/null 2>&1 || true; t1=$(seconds)
    all+=("$(elapsed "$t0" "$t1")")
  done
  echo "${all[*]}"
}

# Warm everything once: dependencies, the project, and a check.
cargo build -q 2>/dev/null
"$oxframe" check >/dev/null 2>&1 || true

# A one-line edit, then cargo build.
sed -i 's/"Measured"/"Measured again"/' src/main.rs
t0=$(seconds); cargo build -q 2>/dev/null; t1=$(seconds)
edit=$(elapsed "$t0" "$t1")

# check on that current build.
check_warm=$(timed "$oxframe" check)

# check right after another edit: includes cargo check and the build.
sed -i 's/"Measured again"/"Measured"/' src/main.rs
t0=$(seconds); "$oxframe" check >/dev/null 2>&1 || true; t1=$(seconds)
check_after=$(elapsed "$t0" "$t1")

# The 30-second video: release build first (not timed), then the render on
# the CPU with the default jobs and with one.
cargo build -q --release 2>/dev/null
bin=target/release/talk
render=$(timed "$bin" render --cpu --out out/talk.mp4)
render_one=$(timed "$bin" render --cpu --jobs 1 --out out/talk1.mp4)
duration=$(ffprobe -v error -show_entries format=duration -of csv=p=0 out/talk.mp4)
jobs=$(python3 -c 'import os; print(min(os.cpu_count() or 1, 8))')
os_name=$(uname -s)
same=$(cmp -s out/talk.mp4 out/talk1.mp4 && echo true || echo false)

if [[ -r /proc/cpuinfo ]] && grep -q 'model name' /proc/cpuinfo; then
  cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //')
else
  # macOS, and Linux on Arm (no "model name" line).
  cpu=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || lscpu 2>/dev/null | sed -n 's/^Model name: *//p' | head -1)
  cpu=${cpu:-$(uname -m)}
fi
threads=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo "?")
rustc=$(rustc --version | cut -d' ' -f2)

python3 - "$version" "$cpu" "$threads" "$rustc" "$edit" "$check_warm" "$check_after" \
  "$render" "$render_one" "$duration" "$jobs" "$same" "$os_name" <<'EOF'
import json, sys, datetime, statistics
(version, cpu, threads, rustc, edit, warm, after, render, one, duration, jobs, same, os_name) = sys.argv[1:]
runs = {name: [float(x) for x in v.split()] for name, v in
        [("checkWarm", warm), ("render30", render), ("render30OneJob", one)]}
warm, render, one = (round(statistics.median(runs[k]), 2) for k in ("checkWarm", "render30", "render30OneJob"))
platform = {"Linux": "Linux", "Darwin": "macOS"}.get(os_name, os_name)
print(json.dumps({
    "version": version,
    "date": datetime.date.today().isoformat(),
    "machine": f"{cpu}, {threads} threads, rustc {rustc}, {platform}",
    "scene": "One scene: a title card, a bullet list and an end card, 30 seconds. oxframe was already built in the same target directory.",
    "editSeconds": float(edit),
    "checkWarmSeconds": float(warm),
    "checkAfterEditSeconds": float(after),
    "render30Seconds": float(render),
    "render30OneJobSeconds": float(one),
    "render30Jobs": int(jobs),
    "render30VideoSeconds": round(float(duration), 2),
    "render30SameBytes": same == "true",
    # Every run of each timing, when RUNS > 1; the fields above are medians.
    "runs": runs,
    "commands": [
        f"cargo build (one-line change, warm dependencies): {float(edit):.2f}s",
        f"oxframe check with a current dev build: {float(warm):.2f}s",
        f"oxframe check immediately after an edit, including cargo check: {float(after):.2f}s",
        f"render of the {float(duration):.1f}-second video on the CPU, {jobs} jobs: {float(render):.2f}s; one job: {float(one):.2f}s",
    ],
}, indent=2))
EOF
