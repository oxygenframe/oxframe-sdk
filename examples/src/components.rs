//! A short explainer built only from components.
//! The scene body stays under 40 lines: the blocks already know the theme,
//! the safe area and how to appear.

use oxframe::prelude::*;

#[oxframe::scene]
pub async fn explainer(view: View) {
    let title = view.add(TitleCard::new("Binary search").subtitle("O(log n)"));
    title.enter().await;
    title.next().await;
    title.exit().await;
    let chapter = view.add(ChapterMarker::new("1  Search"));
    chapter.enter().await;
    chapter.next().await;
    chapter.exit().await;
    let steps = view.add(BulletList::new(["Take the middle", "Compare", "Drop half"]));
    steps.enter().await;
    for _ in 0..steps.len() {
        steps.next().await;
    }
    steps.exit().await;
    let code = view.add(CodeExplainer::new("lo + (hi - lo) / 2").step(1, 1, "the middle"));
    code.enter().await;
    code.next().await;
    code.exit().await;
    let flow = view.add(FlowDiagram::new().edge("Client", "API").edge("API", "DB"));
    flow.enter().await;
    for _ in 0..flow.len() {
        flow.next().await;
    }
    flow.exit().await;
    let card = view.add(Rect::new().size(280.0).fill(Surface).opacity(0.0));
    let call = view.add(Callout::new(card).text("the middle"));
    all((card.fade_in(), call.enter())).await;
    call.next().await;
    all((card.fade_out(), call.exit())).await;
    let lower = view.add(LowerThird::new("Ada").role("walks the half"));
    lower.enter().await;
    lower.next().await;
    lower.exit().await;
    let end = view.add(EndCard::new("That's the search").note("Repeat from the top"));
    end.enter().await;
    end.next().await;
    end.exit().await;
}

pub fn project() -> Project {
    Project::new("components")
        .aspects([Aspect::Landscape, Aspect::Portrait, Aspect::Square])
        .scene(explainer())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use oxframe::Theme;
    use oxframe::feedback::{self, Options};
    use oxframe::lint::Severity;

    use super::project;

    #[test]
    fn every_theme_and_aspect_is_clean() {
        for theme in [
            Theme::dark(),
            Theme::light(),
            Theme::midnight(),
            Theme::paper(),
            Theme::vivid(),
        ] {
            let light = theme.is_light();
            let report = feedback::report(
                project()
                    .theme(theme)
                    .write_meta(false)
                    .inline_meta(BTreeMap::<String, oxframe::protocol::Meta>::new()),
                &Options::default(),
            )
            .unwrap();
            let errors: Vec<_> = report
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| {
                    format!(
                        "{} {} {:.2}..{:.2} {} {}",
                        i.aspect.as_deref().unwrap_or("-"),
                        i.rule,
                        i.time[0],
                        i.time[1],
                        i.node,
                        i.hint
                    )
                })
                .collect::<Vec<_>>();
            assert!(errors.is_empty(), "light={light}\n{}", errors.join("\n"));
        }
    }
}
