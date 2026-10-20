//! Example scenes. Each module exposes `project()`; the binaries in
//! `src/bin` call `project().main()`, so every example supports
//! `cargo run -p oxframe-examples --bin <name> -- render|preview|info`.

pub mod benchmark;
pub mod camera;
pub mod chart;
pub mod code_steps;
pub mod components;
pub mod effects;
pub mod intro;
pub mod layout;
pub mod learn_01_hello;
pub mod learn_02_together;
pub mod learn_03_shapes;
pub mod learn_04_easing;
pub mod learn_05_text;
pub mod learn_06_groups;
pub mod learn_07_editor;
pub mod learn_08_signals;
pub mod learn_09_drawing;
pub mod learn_10_random;
pub mod morph;
pub mod particles;
pub mod quicksort;
pub mod relative;
pub mod shapes;
pub mod showcase;
pub mod svg;
pub mod text;
pub mod theme;
pub mod typography;

/// Builds an example project.
pub type ProjectFn = fn() -> oxframe::Project;

/// All example projects by name (used by the gallery and tests).
pub fn all() -> Vec<(&'static str, ProjectFn)> {
    vec![
        ("intro", intro::project),
        ("camera", camera::project),
        ("code_steps", code_steps::project),
        ("text", text::project),
        ("morph", morph::project),
        ("layout", layout::project),
        ("chart", chart::project),
        ("components", components::project),
        ("particles", particles::project),
        ("quicksort", quicksort::project),
        ("showcase", showcase::project),
        ("effects", effects::project),
        ("shapes", shapes::project),
        ("svg", svg::project),
        ("typography", typography::project),
        ("theme", theme::project),
        ("relative", relative::project),
        ("learn_01_hello", learn_01_hello::project),
        ("learn_02_together", learn_02_together::project),
        ("learn_03_shapes", learn_03_shapes::project),
        ("learn_04_easing", learn_04_easing::project),
        ("learn_05_text", learn_05_text::project),
        ("learn_06_groups", learn_06_groups::project),
        ("learn_07_editor", learn_07_editor::project),
        ("learn_08_signals", learn_08_signals::project),
        ("learn_09_drawing", learn_09_drawing::project),
        ("learn_10_random", learn_10_random::project),
        ("benchmark", benchmark::project),
    ]
}
