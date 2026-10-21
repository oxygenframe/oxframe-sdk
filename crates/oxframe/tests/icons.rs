//! `icon!` through the `icons` feature.
#![cfg(feature = "icons")]

use oxframe::prelude::*;

#[test]
fn icons_from_every_set_are_nodes() {
    let g = oxframe::SceneGraph::new(400, 400);
    g.enter(|| {
        let view = View::current();
        let server = view.add(icon!("lucide:server").size(64.0).color(Accent));
        assert_eq!(server.size(), (64.0, 64.0));
        assert_eq!(server.style(), IconStyle::Stroke);
        assert_eq!(
            view.add(icon!("tabler:heart-filled")).style(),
            IconStyle::Fill
        );
        let github = view.add(icon!("simple:github").color(Color::WHITE));
        assert_eq!(github.style(), IconStyle::Fill);
        let paths = github.paths();
        assert!(!paths.is_empty());
        assert_eq!(paths[0].fill().get(), Color::WHITE);
    });
}
