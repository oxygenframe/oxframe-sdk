//! A card, a title and a caption with no hardcoded colors or sizes.
//! `Project::theme` is the one line that changes the whole picture.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn theme_scene(view: View) {
    let card = view.add(
        Rect::new()
            .size(720.0)
            .radius(Lg)
            .fill(Surface)
            .elevation(Md)
            .name("card"),
    );
    let title = card.add(
        Txt::new()
            .text("Binary search")
            .display()
            .y(-48.0)
            .name("title"),
    );
    card.add(
        Txt::new()
            .text("O(log n)")
            .caption()
            .fill(Muted)
            .y(72.0)
            .name("note"),
    );
    title.fade_in().await;
}

pub fn project() -> Project {
    Project::new("theme")
        .theme(Theme::dark().accent(color!("#68abdf")))
        .scene(theme_scene())
}
