//! The reel on oxframe.dev: scenes from other examples, between a title and
//! an end card. Every frame is drawn by oxframe; nothing is edited by hand.

use oxframe::prelude::*;

use crate::{charts, cube_3d, linear_map, ocean};

#[oxframe::scene]
pub async fn opening(view: View) {
    let title =
        view.add(TitleCard::new("oxframe").subtitle("Motion graphics your AI agent can see"));
    title.enter().await;
    title.next().await;
    wait_for(1.6).await;
    title.exit().await;
}

#[oxframe::scene]
pub async fn closing(view: View) {
    transition(Transition::fade()).await;
    let end = view.add(EndCard::new("").rich(made_by_oxframe()).note("oxframe.dev"));
    end.enter().await;
    end.next().await;
    wait_for(1.6).await;
}

pub fn project() -> Project {
    Project::new("showreel")
        .scene(opening())
        .scene(charts::functions())
        .scene(cube_3d::diagram())
        .scene(linear_map::shear())
        .scene(ocean::ocean())
        .scene(closing())
}
