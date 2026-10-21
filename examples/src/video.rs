//! A clip in the scene: a screen recording in a rounded frame next to its
//! caption, slowed down to show a detail. Needs the `video` feature and
//! ffmpeg; the browser playground shows a placeholder box instead.

use oxframe::prelude::*;

fn asset(name: &str) -> String {
    format!("{}/assets/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[oxframe::scene]
pub async fn clip(view: View) {
    let demo = view.add(
        Video::new()
            .file(asset("demo.mp4"))
            .width(pct(60))
            .radius(24.0)
            .x(-180.0),
    );
    let caption = view.add(
        Txt::new()
            .text("Slow motion")
            .h1()
            .opacity(0.0)
            .next_to(demo, Side::Right),
    );
    demo.play();
    wait_for(1.5).await;
    all((demo.playback_rate().to(0.25, 0.5), caption.fade_in())).await;
    wait_for(1.0).await;
    demo.playback_rate().to(1.0, 0.5).await;
    wait_for(0.5).await;
}

pub fn project() -> Project {
    Project::new("video").scene(clip())
}
