//! Quicksort, step by step: the code with a moving focus, synced to bars that compare, swap and settle.
//!
//! A tour of most of oxframe in one scene: highlighted `Code` with focus,
//! hundreds of coordinated tweens (`all`, `sequence`), seeded random data,
//! `tune!` values you can drag in the inspector (bar count, speed), named
//! eases, markers, glow, gradients and text morphing.
//!
//! Try: change the seed in `project()`, or the `bars` and `speed` tunables.

use oxframe::prelude::*;

const CODE: &str = "fn quicksort(a: &mut [u32], lo: i32, hi: i32) {
    if lo >= hi { return; }
    let p = partition(a, lo, hi);
    quicksort(a, lo, p - 1);
    quicksort(a, p + 1, hi);
}

fn partition(a: &mut [u32], lo: i32, hi: i32) -> i32 {
    let pivot = a[hi];
    let mut i = lo;
    for j in lo..hi {
        if a[j] < pivot {
            a.swap(i, j);
            i += 1;
        }
    }
    a.swap(i, hi);
    i
}";

/// One step of the algorithm, recorded up front and then animated.
#[derive(Clone, Copy)]
enum Step {
    Range { lo: usize, hi: usize },
    Pivot { at: usize },
    Compare { j: usize, i: usize, less: bool },
    Swap { a: usize, b: usize, line: usize },
    Placed { at: usize },
}

fn trace(values: &[u32]) -> Vec<Step> {
    fn sort(a: &mut Vec<u32>, lo: i64, hi: i64, out: &mut Vec<Step>) {
        if lo > hi {
            return;
        }
        let (l, h) = (lo as usize, hi as usize);
        out.push(Step::Range { lo: l, hi: h });
        if lo == hi {
            out.push(Step::Placed { at: l });
            return;
        }
        out.push(Step::Pivot { at: h });
        let pivot = a[h];
        let mut i = l;
        for j in l..h {
            let less = a[j] < pivot;
            out.push(Step::Compare { j, i, less });
            if less {
                if i != j {
                    a.swap(i, j);
                    out.push(Step::Swap {
                        a: i,
                        b: j,
                        line: 13,
                    });
                }
                i += 1;
            }
        }
        if i != h {
            a.swap(i, h);
            out.push(Step::Swap {
                a: i,
                b: h,
                line: 17,
            });
        }
        out.push(Step::Placed { at: i });
        sort(a, lo, i as i64 - 1, out);
        sort(a, i as i64 + 1, hi, out);
    }
    let mut a = values.to_vec();
    let mut out = Vec::new();
    let last = a.len() as i64 - 1;
    sort(&mut a, 0, last, &mut out);
    out
}

const BASE: Color = Color::rgb(0.40, 0.67, 0.87);
const COMPARE: Color = Color::rgb(0.96, 0.55, 0.20);
const PIVOT: Color = Color::rgb(0.96, 0.76, 0.26);
const DONE: Color = Color::rgb(0.18, 0.80, 0.44);

#[derive(Clone, Copy)]
struct Bar {
    group: Group,
    rect: Rect,
}

#[oxframe::scene]
pub async fn quicksort(view: View) {
    let n = tune!("bars", 14.0).clamp(4.0, 28.0) as usize;
    let speed = tune!("speed", 1.0).max(0.1);
    let beat = 0.32 / speed;
    let swap_ease = ease!("swap", presets::IN_OUT_CUBIC);

    // --- title -----------------------------------------------------------
    let title = view.add(
        Txt::new()
            .text("oxframe")
            .font_size(120.0)
            .weight(800.0)
            .y(-60.0),
    );
    wait_for(0.4).await;
    title.morph_to("quicksort", 0.9).await;
    all((title.font_size().to(56.0, 0.8), title.y().to(-470.0, 0.8))).await;
    let subtitle = view.add(
        Txt::new()
            .y(-405.0)
            .font_size(28.0)
            .rich([
                span("partition around a "),
                span("pivot").bold().color(PIVOT),
                span(", then sort both "),
                span("halves").italic().color(BASE),
            ])
            .end(0.0),
    );
    subtitle.end().to(1.0, 0.6).ease(presets::LINEAR).await;

    // --- code panel --------------------------------------------------------
    let panel = view.add(
        Rect::new()
            .width(900.0)
            .height(680.0)
            .radius(24.0)
            .position((-470.0, 60.0))
            .fill(color!("#1b1d23"))
            .shadow(Color::BLACK.with_alpha(0.55), 24.0, (0.0, 18.0))
            .opacity(0.0)
            .scale(0.96),
    );
    let code = view.add(
        Code::new()
            .code(CODE)
            .font_size(23.0)
            .position((-470.0, 60.0))
            .focus(1, 6)
            .opacity(0.0),
    );
    all((
        panel.opacity().to(1.0, 0.5),
        panel.scale().to(Vec2::new(1.0, 1.0), 0.5),
        code.opacity().to(1.0, 0.6),
    ))
    .await;

    // --- bars ---------------------------------------------------------------
    let values: Vec<u32> = (0..n).map(|_| random_range(8.0, 100.0) as u32).collect();
    let (left, width, baseline) = (80.0, 800.0, 300.0);
    let step = width / n as f64;
    let slot = |i: usize| left + step * (i as f64 + 0.5);
    let bar_w = (step * 0.72).min(64.0);

    // Active range behind the bars.
    let range = view.add(
        Rect::new()
            .height(560.0)
            .width(width)
            .radius(14.0)
            .position((left + width / 2.0, baseline - 255.0))
            .fill(Color::WHITE.with_alpha(0.05))
            .opacity(0.0),
    );
    let mut bars = Vec::with_capacity(n);
    for (i, v) in values.iter().enumerate() {
        let group = view.add(Group::new().position((slot(i), baseline)));
        let rect = group.add(
            Rect::new()
                .width(bar_w)
                .height(0.0)
                .radius(6.0)
                .anchor((0.0, 1.0))
                .fill(BASE)
                .fill_gradient(
                    Gradient::linear((0.0, -260.0), (0.0, 260.0))
                        .stops([BASE, BASE.with_alpha(0.55)]),
                ),
        );
        group.add(
            Txt::new()
                .text(v.to_string())
                .family(MONO)
                .font_size((bar_w * 0.45).clamp(12.0, 22.0))
                .fill(Color::WHITE.with_alpha(0.75))
                .y(26.0),
        );
        bars.push((Bar { group, rect }, *v));
    }
    // Bars grow in, left to right.
    stagger(0.04, &bars, |(b, v)| {
        b.rect
            .height()
            .to(f64::from(*v) * 5.0, 0.6)
            .ease(presets::OUT_BACK)
    })
    .await;

    // Pointers `i` and `j` under the bars.
    let pointer = |label: &str, c: Color, row: f64| {
        let g = view.add(
            Group::new()
                .position((slot(0), baseline + 64.0 + row))
                .opacity(0.0),
        );
        g.add(Polygon::new().sides(3.0).size(22.0).fill(c));
        g.add(
            Txt::new()
                .text(label)
                .family(MONO)
                .font_size(22.0)
                .fill(c)
                .y(28.0),
        );
        g
    };
    let pi = pointer("i", PIVOT, 0.0);
    let pj = pointer("j", COMPARE, 56.0);

    let counter = view.add(
        Txt::new()
            .family(MONO)
            .font_size(24.0)
            .text("comparisons 0   swaps 0")
            .fill(Color::WHITE.with_alpha(0.7))
            .position((left + width / 2.0, baseline + 190.0)),
    );

    wait_until("sort").await;

    // --- the algorithm --------------------------------------------------------
    let (mut comparisons, mut swaps) = (0, 0);
    let color = |b: Bar, c: Color, d: f64| {
        all((
            b.rect.fill().to(c, d),
            b.rect.fill_gradient().to(
                Some(Gradient::linear((0.0, -260.0), (0.0, 260.0)).stops([c, c.with_alpha(0.55)])),
                d,
            ),
        ))
    };
    let mut placed = vec![false; n];
    for s in trace(&values) {
        match s {
            Step::Range { lo, hi } => {
                let (x0, x1) = (slot(lo) - step / 2.0, slot(hi) + step / 2.0);
                all((
                    code.focus_lines(1, 3, beat * 0.6),
                    range.opacity().to(1.0, beat),
                    range.x().to((x0 + x1) / 2.0, beat),
                    range.width().to(x1 - x0, beat),
                    pi.x().to(slot(lo), beat),
                    pj.x().to(slot(lo), beat),
                    pi.opacity().to(if lo < hi { 1.0 } else { 0.0 }, beat),
                    pj.opacity().to(if lo < hi { 1.0 } else { 0.0 }, beat),
                ))
                .await;
            }
            Step::Pivot { at } => {
                let b = bars[at].0;
                all((
                    code.focus_lines(8, 10, beat * 0.6),
                    color(b, PIVOT, beat),
                    b.rect.shadow_color().to(PIVOT.with_alpha(0.8), beat),
                    b.rect.shadow_blur().to(14.0, beat),
                ))
                .await;
            }
            Step::Compare { j, i, less } => {
                comparisons += 1;
                counter
                    .text()
                    .set(format!("comparisons {comparisons}   swaps {swaps}"));
                let b = bars[j].0;
                all((
                    code.focus_lines(11, 12, beat * 0.4),
                    pj.x().to(slot(j), beat * 0.5),
                    pi.x().to(slot(i), beat * 0.5),
                    color(b, COMPARE, beat * 0.5),
                ))
                .await;
                // Smaller than the pivot: it will move left of `i`.
                b.group
                    .y()
                    .to(baseline - if less { 18.0 } else { 0.0 }, beat * 0.3)
                    .await;
                all((
                    color(b, BASE, beat * 0.4),
                    b.group.y().to(baseline, beat * 0.4),
                ))
                .await;
            }
            Step::Swap { a, b, line } => {
                swaps += 1;
                counter
                    .text()
                    .set(format!("comparisons {comparisons}   swaps {swaps}"));
                let (ba, bb) = (bars[a].0, bars[b].0);
                let lift = 40.0;
                all((
                    code.focus_lines(line, line, beat * 0.4),
                    ba.group.x().to(slot(b), beat * 1.4).ease(swap_ease),
                    bb.group.x().to(slot(a), beat * 1.4).ease(swap_ease),
                    async move {
                        all((
                            ba.group.y().to(baseline - lift, beat * 0.7),
                            bb.group.y().to(baseline + lift * 0.5, beat * 0.7),
                        ))
                        .await;
                        all((
                            ba.group.y().to(baseline, beat * 0.7),
                            bb.group.y().to(baseline, beat * 0.7),
                        ))
                        .await;
                    },
                ))
                .await;
                bars.swap(a, b);
            }
            Step::Placed { at } => {
                placed[at] = true;
                let b = bars[at].0;
                all((
                    code.focus_lines(17, 18, beat * 0.4),
                    color(b, DONE, beat),
                    b.rect.shadow_color().to(Color::TRANSPARENT, beat),
                    b.rect.scale().to(Vec2::new(1.08, 1.08), beat * 0.5),
                ))
                .await;
                b.rect.scale().to(Vec2::new(1.0, 1.0), beat * 0.5).await;
            }
        }
    }

    // --- finale ---------------------------------------------------------------
    wait_until("finale").await;
    all((
        range.opacity().to(0.0, 0.5),
        pi.opacity().to(0.0, 0.5),
        pj.opacity().to(0.0, 0.5),
        code.focus_lines(0, 0, 0.5),
    ))
    .await;
    stagger(0.05, &bars, |(b, _)| {
        let r = b.rect;
        async move {
            all((
                r.shadow_color().to(DONE.with_alpha(0.9), 0.3),
                r.shadow_blur().to(18.0, 0.3),
            ))
            .await;
            all((
                r.shadow_color().to(Color::TRANSPARENT, 0.5),
                r.shadow_blur().to(0.0, 0.5),
            ))
            .await;
        }
    })
    .await;
    title.morph_to("sorted", 0.8).await;
    wait_for(1.2).await;
}

pub fn project() -> Project {
    Project::new("quicksort").seed(42).scene(quicksort())
}
