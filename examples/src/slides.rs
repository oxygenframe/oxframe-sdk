//! A talk: `oxframe present` stops at every `slide_break` and at the end
//! of each scene, and shows the notes in the presenter window. Rendered,
//! the same scenes play straight through as a video.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn opening(view: View) {
    notes("Introduce yourself. One line on why video from code.");
    let title = view.add(TitleCard::new("Video as code").subtitle("A talk in three stops"));
    title.enter().await;
    title.next().await;
    // The video plays through the stops, so hold before each one: in the
    // presentation the hold is a still frame before it stops anyway.
    wait_for(1.5).await;
    slide_break().await;

    notes("Each point appears on the next click.");
    title.exit().await;
    let points = view.add(BulletList::new([
        "Scenes are async Rust",
        "Every value is a signal",
        "The agent checks its own frames",
    ]));
    points.enter().await;
    // The last point ends the scene, so its stop is the scene's end; the
    // fade into the next scene takes the list away.
    for _ in 0..3 {
        points.next().await;
        wait_for(1.5).await;
        slide_break().await;
    }
}

#[oxframe::scene]
pub async fn closing(view: View) {
    transition(Transition::fade()).await;
    notes("Point to the docs and take questions.");
    let end = view.add(EndCard::new("Questions?").note("oxframe.dev"));
    end.enter().await;
    end.next().await;
    wait_for(0.5).await;
}

pub fn project() -> Project {
    Project::new("slides").scene(opening()).scene(closing())
}
