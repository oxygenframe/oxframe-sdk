//! Lesson 11 — Scenes and transitions.
//!
//! A project is a list of scenes, played in order. A scene that starts with
//! `transition(...)` blends in over the one before it, so one render gives
//! the whole video without cutting it together elsewhere.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn first(view: View) {
    let title = view.add(TitleCard::new("Scene one").subtitle("a title card"));
    title.enter().await;
    wait_for(0.6).await;
}

#[oxframe::scene]
pub async fn second(view: View) {
    // The previous scene holds its last frame while this one slides in.
    // The call waits out the blend; its length comes from the theme.
    transition(Transition::slide(Dir::Left)).await;
    let list = view.add(BulletList::new([
        "Each scene is an async function",
        "Project::scene adds it in order",
        "transition blends it in",
    ]));
    list.enter().await;
    list.next().await;
    list.next().await;
    wait_for(0.4).await;
}

#[oxframe::scene]
pub async fn third(view: View) {
    transition(Transition::fade().duration(0.8)).await;
    let end = view.add(EndCard::new("Three scenes, one video"));
    end.enter().await;
}

// Try: `Transition::wipe(Dir::Up)` or `Transition::zoom_in()` in `second`.

pub fn project() -> Project {
    Project::new("scenes")
        .scene(first())
        .scene(second())
        .scene(third())
}
