//! A release's notes as a short video: title, changes, end card.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn title(view: View) {
    let card = view.add(TitleCard::new("v0.6.0").subtitle("oxygenframe/oxframe"));
    card.enter().await;
    card.next().await;
    wait_for(1.6).await;
}

#[oxframe::scene]
pub async fn changes(view: View) {
    transition(Transition::slide(Dir::Left)).await;
    let list = view.add(BulletList::new([
        "Slide from one scene into the next",
        "Fade, wipe and zoom",
        "The blend is part of the scene",
    ]));
    list.enter().await;
    for _ in 0..list.len() {
        list.next().await;
    }
    wait_for(1.2).await;
}

#[oxframe::scene]
pub async fn end(view: View) {
    transition(Transition::fade()).await;
    let end = view.add(EndCard::new("v0.6.0").note("made with oxframe"));
    end.enter().await;
    end.next().await;
    wait_for(0.6).await;
}

pub fn project() -> Project {
    Project::new("release")
        .scene(title())
        .scene(changes())
        .scene(end())
}
