//! Visual lint over sampled scene graphs. Pure: no rendering, no files.

use oxframe_2d::InspectNode;
use oxframe_core::Color;
use serde::Serialize;

/// How serious a [`Issue`] is. Errors make `oxframe check` exit non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Fails the check.
    Error,
    /// Reported, but the check still succeeds.
    Warning,
}

/// One problem found in a scene.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    /// Rule id, e.g. `"text_overflow"`.
    pub rule: &'static str,
    /// `error` or `warning`.
    pub severity: Severity,
    /// Inclusive time range, in seconds from the start of the project.
    pub time: [f64; 2],
    /// `.name(..)`, or the `add` call site.
    pub node: String,
    /// `file:line` of the `add` call, when known.
    pub source: Option<String>,
    /// What to change.
    pub hint: String,
    /// Frame shape this was found in, when the project declares several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect: Option<String>,
}

/// Thresholds for [`lint`]. See `[check]` in `oxframe.toml`.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Seconds. A node outside the frame for longer than this is reported.
    pub out_persist: f64,
    /// Inset of the safe area, in scene pixels. 0 disables the safe-area warning.
    pub safe_inset: f64,
    /// Minimum WCAG contrast ratio of text against what is behind it.
    pub min_contrast: f64,
    /// A large move that finishes in less than this (seconds) is `too_fast`.
    pub min_motion: f64,
    /// A move counts as large when the center travels this fraction of the
    /// shorter frame side.
    pub motion_fraction: f64,
    /// A node must be at least this big (pixels) for its motion to count.
    pub motion_min_size: f64,
    /// Seconds with no visible change and no soundtrack.
    pub max_still: f64,
    /// Reading speed used by `too_fast` for text.
    pub chars_per_second: f64,
    /// `too_fast` ignores text shorter than this.
    pub min_chars: usize,
    /// Text held at least this long is not `too_fast`, even if a full reading
    /// would take longer. Catches flashes, not short titles.
    pub text_flash: f64,
    /// Gap between samples, used to join hits into one interval.
    pub sample_step: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            out_persist: 0.4,
            safe_inset: 48.0,
            min_contrast: 4.5,
            min_motion: 0.15,
            motion_fraction: 0.15,
            motion_min_size: 64.0,
            max_still: 3.0,
            chars_per_second: 20.0,
            min_chars: 24,
            text_flash: 0.8,
            sample_step: 0.1,
        }
    }
}

/// The graph at one moment.
#[derive(Debug, Clone)]
pub struct Sample {
    /// Seconds from the start of the project.
    pub time: f64,
    /// Nodes, including the root.
    pub nodes: Vec<InspectNode>,
}

/// Runs every rule. `size` is the project frame in pixels.
pub fn lint(
    samples: &[Sample],
    size: (u32, u32),
    background: Color,
    has_audio: bool,
    limits: &Limits,
) -> Vec<Issue> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut issues = Vec::new();
    issues.extend(out_of_frame(samples, size, limits));
    issues.extend(text_overlap(samples, limits));
    issues.extend(text_overflow(samples, limits));
    issues.extend(low_contrast(samples, background, limits));
    issues.extend(too_fast_text(samples, limits));
    issues.extend(too_fast_motion(samples, size, limits));
    if !has_audio {
        issues.extend(dead_time(samples, limits));
    }
    issues.sort_by(|a, b| {
        a.time[0]
            .total_cmp(&b.time[0])
            .then(a.rule.cmp(b.rule))
            .then(a.node.cmp(&b.node))
    });
    issues
}

fn frame_rect(size: (u32, u32), inset: f64) -> [f64; 4] {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    [
        -w / 2.0 + inset,
        -h / 2.0 + inset,
        w / 2.0 - inset,
        h / 2.0 - inset,
    ]
}

fn outside(bounds: [f64; 4], frame: [f64; 4]) -> bool {
    bounds.iter().all(|v| v.is_finite())
        && (bounds[0] < frame[0] - 1.0
            || bounds[1] < frame[1] - 1.0
            || bounds[2] > frame[2] + 1.0
            || bounds[3] > frame[3] + 1.0)
}

fn visible(n: &InspectNode) -> bool {
    n.id != 0 && n.kind != "Group" && n.opacity > 0.05 && n.order != u32::MAX && finite(n.bounds)
}

fn finite(b: [f64; 4]) -> bool {
    b.iter().all(|v| v.is_finite())
}

fn is_text(n: &InspectNode) -> bool {
    n.text.as_ref().is_some_and(|t| !t.trim().is_empty()) && (n.kind == "Txt" || n.kind == "Code")
}

struct Hit<'a> {
    time: f64,
    node: &'a InspectNode,
}

/// `(start, end, first sample, last sample)`, one span per stay of a node.
fn spans<'a>(hits: &[Hit<'a>], step: f64) -> Vec<(f64, f64, &'a InspectNode, &'a InspectNode)> {
    let mut hits: Vec<&Hit<'a>> = hits.iter().collect();
    hits.sort_by(|a, b| a.node.id.cmp(&b.node.id).then(a.time.total_cmp(&b.time)));
    let mut out: Vec<(f64, f64, &'a InspectNode, &'a InspectNode)> = Vec::new();
    for hit in hits {
        if let Some((_, end, _, last)) = out.last_mut()
            && last.id == hit.node.id
            && hit.time - *end <= step * 1.5
        {
            *end = hit.time;
            *last = hit.node;
        } else {
            out.push((hit.time, hit.time, hit.node, hit.node));
        }
    }
    out
}

fn moved(a: &InspectNode, b: &InspectNode) -> f64 {
    let (x0, y0) = center(a);
    let (x1, y1) = center(b);
    ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt()
}

fn persist(start: f64, end: f64, step: f64) -> f64 {
    (end - start) + step
}

fn out_of_frame(samples: &[Sample], size: (u32, u32), limits: &Limits) -> Vec<Issue> {
    let frame = frame_rect(size, 0.0);
    let safe = frame_rect(size, limits.safe_inset.max(0.0));
    let mut frame_hits = Vec::new();
    let mut safe_hits = Vec::new();
    for s in samples {
        // Through a camera, leaving the frame is the framing (a zoom onto a
        // detail), not a mistake.
        for n in s.nodes.iter().filter(|n| visible(n) && !n.in_camera) {
            if outside(n.bounds, frame) {
                frame_hits.push(Hit {
                    time: s.time,
                    node: n,
                });
            } else if limits.safe_inset > 0.0 && outside(n.bounds, safe) {
                safe_hits.push(Hit {
                    time: s.time,
                    node: n,
                });
            }
        }
    }
    let mut issues = Vec::new();
    // A node traveling through the edge (an entrance, or particles leaving)
    // is not stuck. Only a node that stays put outside the frame is.
    for (start, end, first, last) in spans(&frame_hits, limits.sample_step) {
        if persist(start, end, limits.sample_step) + 1e-6 < limits.out_persist
            || moved(first, last) > 24.0
        {
            continue;
        }
        issues.push(issue(
            "out_of_frame",
            Severity::Error,
            [start, end],
            last,
            "The node sits outside the frame. Move it inward or make it smaller.",
        ));
    }
    for (start, end, _, last) in spans(&safe_hits, limits.sample_step) {
        if persist(start, end, limits.sample_step) + 1e-6 < limits.out_persist {
            continue;
        }
        issues.push(issue(
            "out_of_frame",
            Severity::Warning,
            [start, end],
            last,
            format!(
                "The node is outside the safe area ({:.0} px from the edge). Move it inward.",
                limits.safe_inset
            ),
        ));
    }
    issues
}

fn overlap_area(a: [f64; 4], b: [f64; 4]) -> f64 {
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    w * h
}

fn box_area(b: [f64; 4]) -> f64 {
    (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0)
}

fn text_overlap(samples: &[Sample], limits: &Limits) -> Vec<Issue> {
    let mut hits = Vec::new();
    for s in samples {
        let texts: Vec<_> = s
            .nodes
            .iter()
            .filter(|n| visible(n) && is_text(n))
            .collect();
        for i in 0..texts.len() {
            for j in i + 1..texts.len() {
                let area = overlap_area(texts[i].bounds, texts[j].bounds);
                let smaller = box_area(texts[i].bounds).min(box_area(texts[j].bounds));
                // Line boxes overlap a little when two captions are stacked.
                // Report it when one text actually covers the other.
                if area < 8.0 || area < smaller * 0.3 {
                    continue;
                }
                let (top, other) = if texts[i].order >= texts[j].order {
                    (texts[i], texts[j])
                } else {
                    (texts[j], texts[i])
                };
                hits.push((s.time, top, other.name.clone()));
            }
        }
    }
    let mut issues = Vec::new();
    let mut group: Option<(f64, f64, &InspectNode, String)> = None;
    let flush = |group: &mut Option<(f64, f64, &InspectNode, String)>, issues: &mut Vec<Issue>| {
        if let Some((start, end, node, other)) = group.take()
            && persist(start, end, limits.sample_step) + 1e-6 >= limits.out_persist
        {
            issues.push(issue(
                "text_overlap",
                Severity::Error,
                [start, end],
                node,
                format!(
                    "Text overlaps `{other}`. Move one of them so they no longer cover each other."
                ),
            ));
        }
    };
    for (time, node, other) in &hits {
        let same = group.as_ref().is_some_and(|(start, end, n, o)| {
            n.id == node.id
                && o == other
                && *time - *end <= limits.sample_step * 1.5
                && *time >= *start
        });
        if same {
            if let Some(g) = group.as_mut() {
                g.1 = *time;
            }
        } else {
            flush(&mut group, &mut issues);
            group = Some((*time, *time, node, other.clone()));
        }
    }
    flush(&mut group, &mut issues);
    let _ = limits;
    issues
}

fn protrusion(node: &InspectNode, parent: &InspectNode) -> f64 {
    let left = (parent.bounds[0] - node.bounds[0]).max(0.0);
    let right = (node.bounds[2] - parent.bounds[2]).max(0.0);
    let top = (parent.bounds[1] - node.bounds[1]).max(0.0);
    let bottom = (node.bounds[3] - parent.bounds[3]).max(0.0);
    left.max(right).max(top).max(bottom)
}

fn text_overflow(samples: &[Sample], limits: &Limits) -> Vec<Issue> {
    let mut hits: Vec<(f64, &InspectNode, f64)> = Vec::new();
    for s in samples {
        for n in s.nodes.iter().filter(|n| visible(n) && is_text(n)) {
            let mut px = n.text_overflow;
            if n.parent_kind == Some("Layout")
                && let Some(parent) = n.parent.and_then(|id| s.nodes.iter().find(|p| p.id == id))
            {
                px = px.max(protrusion(n, parent));
            }
            if px > 2.0 {
                hits.push((s.time, n, px));
            }
        }
    }
    grouped(hits, limits, "text_overflow", Severity::Error, |px| {
        format!("Text is {px:.0} px wider than its block. Lower font_size or raise max_width.")
    })
}

fn backdrop(sample: &Sample, text: &InspectNode, background: Color) -> Color {
    let cx = (text.bounds[0] + text.bounds[2]) / 2.0;
    let cy = (text.bounds[1] + text.bounds[3]) / 2.0;
    let mut bg = background;
    for n in &sample.nodes {
        if n.id == text.id || n.order >= text.order || n.fill.a < 0.05 || !visible(n) {
            continue;
        }
        if is_text(n) {
            continue;
        }
        let b = n.bounds;
        if cx >= b[0] && cx <= b[2] && cy >= b[1] && cy <= b[3] {
            bg = over(n.fill, bg);
        }
    }
    Color::rgb(bg.r, bg.g, bg.b)
}

fn over(src: Color, dst: Color) -> Color {
    let a = src.a + dst.a * (1.0 - src.a);
    if a <= 1e-4 {
        return Color::TRANSPARENT;
    }
    let mix = |s: f32, d: f32| (s * src.a + d * dst.a * (1.0 - src.a)) / a;
    Color::rgba(mix(src.r, dst.r), mix(src.g, dst.g), mix(src.b, dst.b), a)
}

fn channel(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn contrast(a: Color, b: Color) -> f64 {
    let lum = |c: Color| 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
    let (l1, l2) = (lum(a), lum(b));
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    f64::from((hi + 0.05) / (lo + 0.05))
}

fn low_contrast(samples: &[Sample], background: Color, limits: &Limits) -> Vec<Issue> {
    let mut hits: Vec<(f64, &InspectNode, f64)> = Vec::new();
    for s in samples {
        for n in s.nodes.iter().filter(|n| visible(n) && is_text(n)) {
            if n.fill.a * (n.opacity as f32) < 0.8 {
                continue;
            }
            let ratio = contrast(n.fill, backdrop(s, n, background));
            if ratio + 1e-6 < limits.min_contrast {
                hits.push((s.time, n, ratio));
            }
        }
    }
    grouped(hits, limits, "low_contrast", Severity::Error, |ratio| {
        format!(
            "Contrast with the background is {ratio:.1}:1, below {:.1}:1. Change the text color or what is behind it.",
            limits.min_contrast
        )
    })
}

fn too_fast_text(samples: &[Sample], limits: &Limits) -> Vec<Issue> {
    let mut seen: Vec<(u32, f64, f64, usize, &InspectNode)> = Vec::new();
    for s in samples {
        for n in s.nodes.iter().filter(|n| visible(n) && is_text(n)) {
            let chars = n.text.as_deref().unwrap_or("").chars().count();
            if let Some(span) = seen.iter_mut().rev().find(|span| span.0 == n.id) {
                if s.time - span.2 <= limits.sample_step * 1.5 {
                    span.2 = s.time;
                    span.3 = span.3.max(chars);
                } else {
                    seen.push((n.id, s.time, s.time, chars, n));
                }
            } else {
                seen.push((n.id, s.time, s.time, chars, n));
            }
        }
    }
    let mut issues = Vec::new();
    for (_, start, end, chars, node) in seen {
        if chars < limits.min_chars {
            continue;
        }
        let visible_for = persist(start, end, limits.sample_step);
        let needed = chars as f64 / limits.chars_per_second.max(1.0);
        if visible_for + 1e-6 < needed && visible_for < limits.text_flash {
            issues.push(issue(
                "too_fast",
                Severity::Error,
                [start, end],
                node,
                format!(
                    "The text is on screen for {visible_for:.1} s but needs about {needed:.1} s to be read. Hold it longer."
                ),
            ));
        }
    }
    issues
}

fn center(n: &InspectNode) -> (f64, f64) {
    (
        (n.bounds[0] + n.bounds[2]) / 2.0,
        (n.bounds[1] + n.bounds[3]) / 2.0,
    )
}

fn too_fast_motion(samples: &[Sample], size: (u32, u32), limits: &Limits) -> Vec<Issue> {
    let large = limits.motion_fraction * f64::from(size.0.min(size.1));
    #[derive(Clone, Copy)]
    struct Run<'a> {
        start: f64,
        end: f64,
        x0: f64,
        y0: f64,
        node: &'a InspectNode,
    }
    let mut open: Vec<Run<'_>> = Vec::new();
    let mut issues = Vec::new();
    let flush = |run: Run<'_>, issues: &mut Vec<Issue>| {
        let (x, y) = center(run.node);
        let dist = ((x - run.x0).powi(2) + (y - run.y0).powi(2)).sqrt();
        let dur = run.end - run.start;
        if dur + 1e-6 < limits.min_motion && dist >= large {
            issues.push(issue(
                "too_fast",
                Severity::Warning,
                [run.start, run.end],
                run.node,
                format!(
                    "The node travels {dist:.0} px in {dur:.2} s. Slow the move so it lasts at least {:.2} s.",
                    limits.min_motion
                ),
            ));
        }
    };
    for s in samples {
        let mut still = vec![true; open.len()];
        for n in s.nodes.iter().filter(|n| visible(n)) {
            let (w, h) = (n.bounds[2] - n.bounds[0], n.bounds[3] - n.bounds[1]);
            if w.max(h) < limits.motion_min_size {
                continue;
            }
            let (x, y) = center(n);
            if let Some(i) = open.iter().position(|r| r.node.id == n.id) {
                let prev = center(open[i].node);
                let step = ((x - prev.0).powi(2) + (y - prev.1).powi(2)).sqrt();
                if step > 2.0 && s.time - open[i].end <= limits.sample_step * 1.5 {
                    open[i].end = s.time;
                    open[i].node = n;
                    still[i] = false;
                }
            } else {
                open.push(Run {
                    start: s.time,
                    end: s.time,
                    x0: x,
                    y0: y,
                    node: n,
                });
            }
        }
        let mut i = open.len();
        while i > 0 {
            i -= 1;
            if still.get(i).copied().unwrap_or(true) && open[i].end < s.time {
                flush(open.remove(i), &mut issues);
            }
        }
    }
    for run in open {
        flush(run, &mut issues);
    }
    issues
}

fn dead_time(samples: &[Sample], limits: &Limits) -> Vec<Issue> {
    if samples.len() < 2 {
        return Vec::new();
    }
    let sig = |s: &Sample| -> String {
        let mut out = String::new();
        for n in s.nodes.iter().filter(|n| visible(n)) {
            let b = n.bounds;
            out.push_str(&format!(
                "{}:{:.0}:{:.0}:{:.0}:{:.0}:{:.2}:{}:{}",
                n.id,
                b[0],
                b[1],
                b[2],
                b[3],
                n.opacity,
                n.text.as_deref().unwrap_or(""),
                n.fill
            ));
        }
        out
    };
    let mut issues = Vec::new();
    let mut start = samples[0].time;
    let mut prev = sig(&samples[0]);
    let mut last = samples[0].time;
    for s in &samples[1..] {
        let now = sig(s);
        if now != prev {
            let dur = last - start;
            if dur + 1e-6 >= limits.max_still {
                issues.push(Issue {
                    rule: "dead_time",
                    severity: Severity::Warning,
                    time: [start, last],
                    node: "scene".into(),
                    source: None,
                    hint: format!(
                        "Nothing changes for {dur:.1} s. Shorten the pause or add motion."
                    ),
                    aspect: None,
                });
            }
            start = s.time;
            prev = now;
        }
        last = s.time;
    }
    let dur = last - start;
    if dur + 1e-6 >= limits.max_still {
        issues.push(Issue {
            rule: "dead_time",
            severity: Severity::Warning,
            time: [start, last],
            node: "scene".into(),
            source: None,
            hint: format!("Nothing changes for {dur:.1} s. Shorten the pause or add motion."),
            aspect: None,
        });
    }
    issues
}

fn issue(
    rule: &'static str,
    severity: Severity,
    time: [f64; 2],
    node: &InspectNode,
    hint: impl Into<String>,
) -> Issue {
    Issue {
        rule,
        severity,
        time,
        node: node.name.clone(),
        source: node.source.clone(),
        hint: hint.into(),
        aspect: None,
    }
}

fn grouped(
    hits: Vec<(f64, &InspectNode, f64)>,
    limits: &Limits,
    rule: &'static str,
    severity: Severity,
    hint: impl Fn(f64) -> String,
) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut group: Option<(f64, f64, &InspectNode, f64)> = None;
    let flush = |g: &mut Option<(f64, f64, &InspectNode, f64)>, issues: &mut Vec<Issue>| {
        if let Some((start, end, node, px)) = g.take()
            && persist(start, end, limits.sample_step) + 1e-6 >= limits.out_persist.min(0.2)
        {
            issues.push(issue(rule, severity, [start, end], node, hint(px)));
        }
    };
    for (time, node, px) in hits {
        let same = group.as_ref().is_some_and(|(_, end, n, _)| {
            n.id == node.id && time - *end <= limits.sample_step * 1.5
        });
        if same {
            if let Some(g) = group.as_mut() {
                g.1 = time;
                g.3 = g.3.max(px);
            }
        } else {
            flush(&mut group, &mut issues);
            group = Some((time, time, node, px));
        }
    }
    flush(&mut group, &mut issues);
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32, kind: &str, bounds: [f64; 4]) -> InspectNode {
        InspectNode {
            id,
            parent: Some(0),
            kind: match kind {
                "Txt" => "Txt",
                "Code" => "Code",
                "Rect" => "Rect",
                _ => "Rect",
            },
            name: format!("n{id}"),
            source: Some(format!("src/main.rs:{id}")),
            bounds,
            opacity: 1.0,
            order: id,
            text: None,
            fill: Color::WHITE,
            text_overflow: 0.0,
            parent_kind: Some("Group"),
            in_camera: false,
        }
    }

    fn text(id: u32, body: &str, bounds: [f64; 4]) -> InspectNode {
        let mut n = node(id, "Txt", bounds);
        n.text = Some(body.into());
        n
    }

    fn at(time: f64, nodes: Vec<InspectNode>) -> Sample {
        Sample { time, nodes }
    }

    fn limits() -> Limits {
        Limits {
            sample_step: 0.1,
            ..Limits::default()
        }
    }

    #[test]
    fn flags_a_node_parked_outside_the_frame() {
        let outside = node(1, "Rect", [900.0, -10.0, 1100.0, 10.0]);
        let samples: Vec<_> = (0..8)
            .map(|i| at(i as f64 * 0.1, vec![outside.clone()]))
            .collect();
        let issues = lint(&samples, (1920, 1080), Color::BLACK, false, &limits());
        assert!(
            issues
                .iter()
                .any(|i| i.rule == "out_of_frame" && i.severity == Severity::Error)
        );
    }

    #[test]
    fn a_camera_framing_out_a_node_is_not_a_mistake() {
        let mut outside = node(1, "Rect", [900.0, -10.0, 1100.0, 10.0]);
        outside.in_camera = true;
        let samples: Vec<_> = (0..8)
            .map(|i| at(i as f64 * 0.1, vec![outside.clone()]))
            .collect();
        let issues = lint(&samples, (1920, 1080), Color::BLACK, false, &limits());
        assert!(issues.iter().all(|i| i.rule != "out_of_frame"));
    }

    #[test]
    fn ignores_a_brief_exit() {
        let outside = node(1, "Rect", [900.0, -10.0, 1100.0, 10.0]);
        let inside = node(1, "Rect", [-50.0, -10.0, 50.0, 10.0]);
        let mut samples = vec![at(0.0, vec![outside.clone()]), at(0.1, vec![outside])];
        samples.push(at(0.2, vec![inside]));
        let issues = lint(&samples, (1920, 1080), Color::BLACK, false, &limits());
        assert!(issues.iter().all(|i| i.rule != "out_of_frame"));
    }

    #[test]
    fn flags_overlapping_text() {
        let a = text(1, "Hello", [-40.0, -10.0, 40.0, 10.0]);
        let b = text(2, "World", [-20.0, -10.0, 60.0, 10.0]);
        let samples: Vec<_> = (0..6)
            .map(|i| at(i as f64 * 0.1, vec![a.clone(), b.clone()]))
            .collect();
        let issues = lint(&samples, (1920, 1080), Color::BLACK, false, &limits());
        assert!(issues.iter().any(|i| i.rule == "text_overlap"));
    }

    #[test]
    fn flags_text_wider_than_max_width() {
        let mut n = text(1, "a long label", [-100.0, -10.0, 100.0, 10.0]);
        n.text_overflow = 140.0;
        let samples: Vec<_> = (0..5)
            .map(|i| at(i as f64 * 0.1, vec![n.clone()]))
            .collect();
        let issues = lint(&samples, (1920, 1080), Color::BLACK, false, &limits());
        assert!(issues.iter().any(|i| i.rule == "text_overflow"));
    }

    #[test]
    fn flags_white_text_on_white() {
        let mut n = text(1, "Nope", [-40.0, -10.0, 40.0, 10.0]);
        n.fill = Color::WHITE;
        let mut bg = node(2, "Rect", [-200.0, -100.0, 200.0, 100.0]);
        bg.fill = Color::WHITE;
        bg.order = 0;
        n.order = 1;
        let samples: Vec<_> = (0..5)
            .map(|i| at(i as f64 * 0.1, vec![bg.clone(), n.clone()]))
            .collect();
        let issues = lint(&samples, (1920, 1080), Color::WHITE, false, &limits());
        assert!(issues.iter().any(|i| i.rule == "low_contrast"));
    }

    #[test]
    fn white_on_dark_is_fine() {
        let n = text(1, "Ok", [-20.0, -10.0, 20.0, 10.0]);
        let samples = vec![at(0.0, vec![n.clone()]), at(0.5, vec![n])];
        let bg = Color::from_rgba8(0x14, 0x14, 0x14, 0xff);
        let issues = lint(&samples, (1920, 1080), bg, false, &limits());
        assert!(issues.iter().all(|i| i.rule != "low_contrast"));
    }

    #[test]
    fn flags_text_that_cannot_be_read_in_time() {
        let body = "This sentence is definitely long enough to need a moment.";
        let n = text(1, body, [-200.0, -20.0, 200.0, 20.0]);
        let samples = vec![at(0.0, vec![n.clone()]), at(0.2, vec![n])];
        let issues = lint(&samples, (1920, 1080), Color::BLACK, false, &limits());
        assert!(
            issues
                .iter()
                .any(|i| i.rule == "too_fast" && i.severity == Severity::Error)
        );
    }

    #[test]
    fn flags_a_large_jump_and_a_long_pause() {
        let a = node(1, "Rect", [-400.0, -40.0, -200.0, 40.0]);
        let b = node(1, "Rect", [200.0, -40.0, 400.0, 40.0]);
        let mut samples = vec![at(0.0, vec![a]), at(0.1, vec![b.clone()])];
        for i in 2..40 {
            samples.push(at(i as f64 * 0.1, vec![b.clone()]));
        }
        let issues = lint(&samples, (1920, 1080), Color::BLACK, false, &limits());
        assert!(
            issues
                .iter()
                .any(|i| i.rule == "too_fast" && i.severity == Severity::Warning)
        );
        assert!(issues.iter().any(|i| i.rule == "dead_time"));
    }

    #[test]
    fn soundtrack_suppresses_dead_time() {
        let n = node(1, "Rect", [-10.0, -10.0, 10.0, 10.0]);
        let samples: Vec<_> = (0..40)
            .map(|i| at(i as f64 * 0.1, vec![n.clone()]))
            .collect();
        let issues = lint(&samples, (1920, 1080), Color::BLACK, true, &limits());
        assert!(issues.iter().all(|i| i.rule != "dead_time"));
    }
}
