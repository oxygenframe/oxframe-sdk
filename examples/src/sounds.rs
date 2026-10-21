//! A beat that plays with the picture, and a tone that ducks under it.

use oxframe::prelude::*;

fn asset(name: &str) -> String {
    format!("{}/assets/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[oxframe::scene]
pub async fn sounds(view: View) {
    let dot = view.add(Circle::new().size(16.0).fill(color!("#e13238")));
    sound(asset("click.wav")).volume(0.8).trim(0.0, 0.18).play();
    dot.size().to(140.0, 0.18).await;
    dot.size().to(28.0, 0.35).await;
    sound(asset("click.wav"))
        .volume(0.5)
        .fade_out(0.04)
        .trim(0.0, 0.18)
        .priority()
        .play();
    dot.size().to(140.0, 0.18).await;
    wait_for(0.4).await;
}

pub fn project() -> Project {
    Project::new("sounds")
        .size(960, 540)
        .fps(60)
        .audio(
            Audio::track(asset("tone.wav"))
                .duck_under_voice()
                .volume(0.4)
                .master_fade(0.05, 0.15),
        )
        .scene(sounds())
}
