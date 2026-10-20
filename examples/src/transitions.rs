//! Slide from a title into an end card.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn intro(view: View) {
    let title = view.add(TitleCard::new("Scene one"));
    title.enter().await;
    wait_for(0.4).await;
}

#[oxframe::scene]
pub async fn outro(view: View) {
    transition(Transition::slide(Dir::Left)).await;
    let end = view.add(EndCard::new("Thanks for watching"));
    end.enter().await;
}

pub fn project() -> Project {
    Project::new("transitions").scene(intro()).scene(outro())
}
