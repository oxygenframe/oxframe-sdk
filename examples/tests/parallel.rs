//! `render --jobs`: segments in parallel give the same file as one job.

use std::path::Path;
use std::process::Command;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn render(out: &Path, jobs: u32) {
    let status = Command::new(env!("CARGO_BIN_EXE_charts"))
        .args([
            "render", "--cpu", "--size", "320x180", "--range", "0..5", "--jobs",
        ])
        .arg(jobs.to_string())
        .arg("--out")
        .arg(out)
        .status()
        .expect("run the charts example");
    assert!(status.success(), "render with {jobs} jobs failed");
}

#[test]
fn any_number_of_jobs_gives_the_same_bytes() {
    if !ffmpeg_available() {
        eprintln!("skipped: ffmpeg is not installed");
        return;
    }
    let dir = std::env::temp_dir().join(format!("oxframe-parallel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (one, three) = (dir.join("one.mp4"), dir.join("three.mp4"));
    // Five seconds: three segments.
    render(&one, 1);
    render(&three, 3);
    let (a, b) = (std::fs::read(&one).unwrap(), std::fs::read(&three).unwrap());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!a.is_empty());
    assert!(
        a == b,
        "--jobs 1 and --jobs 3 differ ({} vs {} bytes)",
        a.len(),
        b.len()
    );
}
