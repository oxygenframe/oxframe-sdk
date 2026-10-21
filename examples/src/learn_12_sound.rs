//! Lesson 12 — Sound and voice.
//!
//! A sound starts at the frame where the scene starts it, so it stays in
//! sync when you change the timing. A soundtrack plays under the whole
//! project and ducks when a voice speaks.

use oxframe::prelude::*;

fn asset(name: &str) -> String {
    format!("{}/assets/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[oxframe::scene]
pub async fn sound_lesson(view: View) {
    let dot = view.add(Circle::new().size(24.0).fill(Accent));

    // A click on the beat: start it, then animate.
    sound(asset("click.wav")).trim(0.0, 0.18).play();
    dot.size().to(140.0, 0.2).await;
    dot.size().to(24.0, 0.3).await;

    // A spoken line. Without a speech provider its length is estimated,
    // which is enough to time the picture to it.
    let line = voice("Sound follows the picture").start();
    line.at_word("follows").await;
    dot.size().to(200.0, 0.3).await;
    line.await;
}

// Try: move the second `dot.size()` before `line.at_word(..)`.

pub fn project() -> Project {
    Project::new("sound")
        .audio(
            Audio::track(asset("tone.wav"))
                .volume(0.4)
                .duck_under_voice(),
        )
        .scene(sound_lesson())
}
