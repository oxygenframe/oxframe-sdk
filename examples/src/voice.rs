//! A spoken line. The circle grows when the second word starts.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn spoken(view: View) {
    let dot = view.add(Circle::new().size(24.0).fill(color!("#e13238")));
    let line = voice("one two three").start();
    line.at_word("two").await;
    dot.size().to(160.0, 0.3).await;
    line.await;
}

pub fn project() -> Project {
    Project::new("voice").size(960, 540).fps(60).scene(spoken())
}
