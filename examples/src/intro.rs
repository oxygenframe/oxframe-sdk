//! A square slides from left to right and turns into a circle in 1 s.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn intro(view: View) {
    let square = view.add(Rect::new().size(200.0).x(-300.0).fill(color!("#e13238")));
    all((
        square.x().to(300.0, 1.0),
        square.radius().to(100.0, 1.0),
        square.fill().to(color!("#68abdf"), 1.0),
    ))
    .await;
}

pub fn project() -> Project {
    Project::new("intro").scene(intro())
}
