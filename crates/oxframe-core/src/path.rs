//! Morphing between arbitrary [`BezPath`]s.
//!
//! Both paths are normalized to closed or open runs of cubic segments, the
//! runs are paired up, segment counts are equalized by splitting the longest
//! segments, and for closed runs the start point and winding direction are
//! chosen to minimize the distance between corresponding points. The aligned
//! pair is cached, so a tween aligns once and then only interpolates.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};

use kurbo::{BezPath, CubicBez, ParamCurve, PathEl, Point, Vec2};

use crate::Lerp;

#[derive(Debug, Clone, PartialEq)]
struct Run {
    segs: Vec<CubicBez>,
    closed: bool,
}

impl Run {
    fn start(&self) -> Point {
        self.segs.first().map_or(Point::ZERO, |s| s.p0)
    }

    fn centroid(&self) -> Point {
        if self.segs.is_empty() {
            return Point::ZERO;
        }
        let sum = self
            .segs
            .iter()
            .fold(Vec2::ZERO, |acc, s| acc + s.p0.to_vec2());
        (sum / self.segs.len() as f64).to_point()
    }

    /// Signed area of the control polygon; the sign gives the direction.
    fn signed_area(&self) -> f64 {
        self.segs
            .iter()
            .map(|s| {
                let pts = [s.p0, s.p1, s.p2, s.p3];
                pts.windows(2)
                    .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
                    .sum::<f64>()
            })
            .sum::<f64>()
            / 2.0
    }

    fn reversed(&self) -> Run {
        Run {
            segs: self
                .segs
                .iter()
                .rev()
                .map(|s| CubicBez::new(s.p3, s.p2, s.p1, s.p0))
                .collect(),
            closed: self.closed,
        }
    }

    fn rotated(&self, k: usize) -> Run {
        let mut segs = self.segs.clone();
        segs.rotate_left(k);
        Run {
            segs,
            closed: self.closed,
        }
    }

    /// A degenerate run of `n` zero-length segments at `p`.
    fn point(p: Point, n: usize, closed: bool) -> Run {
        Run {
            segs: vec![CubicBez::new(p, p, p, p); n.max(1)],
            closed,
        }
    }

    /// Splits the longest segments until there are `n`.
    fn split_to(&mut self, n: usize) {
        while self.segs.len() < n {
            let (i, _) = self
                .segs
                .iter()
                .enumerate()
                .map(|(i, s)| (i, s.p0.distance(s.p3) + 1e-9 * i as f64))
                .fold(
                    (0, f64::MIN),
                    |best, cur| if cur.1 > best.1 { cur } else { best },
                );
            let s = self.segs[i];
            let (a, b) = (s.subsegment(0.0..0.5), s.subsegment(0.5..1.0));
            self.segs.splice(i..=i, [a, b]);
        }
    }

    fn to_path(&self, out: &mut BezPath) {
        let Some(first) = self.segs.first() else {
            return;
        };
        out.move_to(first.p0);
        for s in &self.segs {
            out.curve_to(s.p1, s.p2, s.p3);
        }
        if self.closed {
            out.close_path();
        }
    }
}

fn line(a: Point, b: Point) -> CubicBez {
    CubicBez::new(a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b)
}

fn runs(path: &BezPath) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut cur: Option<Run> = None;
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    let finish = |cur: &mut Option<Run>, out: &mut Vec<Run>| {
        if let Some(r) = cur.take()
            && !r.segs.is_empty()
        {
            out.push(r);
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                finish(&mut cur, &mut out);
                cur = Some(Run {
                    segs: Vec::new(),
                    closed: false,
                });
                (start, last) = (p, p);
            }
            PathEl::LineTo(p) => {
                cur.get_or_insert_with(|| Run {
                    segs: Vec::new(),
                    closed: false,
                })
                .segs
                .push(line(last, p));
                last = p;
            }
            PathEl::QuadTo(c, p) => {
                let q = kurbo::QuadBez::new(last, c, p).raise();
                cur.get_or_insert_with(|| Run {
                    segs: Vec::new(),
                    closed: false,
                })
                .segs
                .push(q);
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                cur.get_or_insert_with(|| Run {
                    segs: Vec::new(),
                    closed: false,
                })
                .segs
                .push(CubicBez::new(last, a, b, p));
                last = p;
            }
            PathEl::ClosePath => {
                if let Some(r) = cur.as_mut() {
                    if last.distance(start) > 1e-9 {
                        r.segs.push(line(last, start));
                    }
                    r.closed = true;
                }
                finish(&mut cur, &mut out);
                last = start;
            }
        }
    }
    finish(&mut cur, &mut out);
    out
}

/// Makes `b` correspond point-by-point to `a` (same counts, best start).
fn align_run(a: &mut Run, b: &mut Run) {
    let n = a.segs.len().max(b.segs.len());
    a.split_to(n);
    b.split_to(n);
    if !(a.closed && b.closed) {
        return;
    }
    if a.signed_area() * b.signed_area() < 0.0 {
        *b = b.reversed();
    }
    let cost = |k: usize| -> f64 {
        (0..n)
            .map(|i| a.segs[i].p0.distance_squared(b.segs[(i + k) % n].p0))
            .sum()
    };
    let best = (0..n)
        .map(|k| (k, cost(k)))
        .fold(
            (0, f64::MAX),
            |best, cur| if cur.1 < best.1 { cur } else { best },
        )
        .0;
    *b = b.rotated(best);
}

fn align(from: &BezPath, to: &BezPath) -> (Vec<Run>, Vec<Run>) {
    let mut a = runs(from);
    let mut b = runs(to);
    // Pair runs by order; a missing partner grows from / shrinks to a point
    // at the centroid of the existing run.
    let n = a.len().max(b.len());
    for i in 0..n {
        match (a.get(i), b.get(i)) {
            (Some(r), None) => b.push(Run::point(r.centroid(), r.segs.len(), r.closed)),
            (None, Some(r)) => a.push(Run::point(r.centroid(), r.segs.len(), r.closed)),
            _ => {}
        }
    }
    for (ra, rb) in a.iter_mut().zip(b.iter_mut()) {
        if ra.closed != rb.closed {
            ra.closed = true;
            rb.closed = true;
            // Close the open run back to its start so the shapes agree.
            for r in [&mut *ra, &mut *rb] {
                let (s, e) = (r.start(), r.segs.last().map_or(r.start(), |c| c.p3));
                if s.distance(e) > 1e-9 {
                    r.segs.push(line(e, s));
                }
            }
        }
        align_run(ra, rb);
    }
    (a, b)
}

fn path_hash(p: &BezPath, h: &mut impl Hasher) {
    for el in p.elements() {
        let pts: &[Point] = match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => std::slice::from_ref(p),
            PathEl::QuadTo(a, b) => &[*a, *b],
            PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
            PathEl::ClosePath => &[],
        };
        std::mem::discriminant(el).hash(h);
        for p in pts {
            p.x.to_bits().hash(h);
            p.y.to_bits().hash(h);
        }
    }
}

type Aligned = (u64, std::rc::Rc<(Vec<Run>, Vec<Run>)>);

thread_local! {
    static CACHE: RefCell<Vec<Aligned>> = const { RefCell::new(Vec::new()) };
}

const CACHE_SIZE: usize = 16;

fn aligned(from: &BezPath, to: &BezPath) -> std::rc::Rc<(Vec<Run>, Vec<Run>)> {
    // `DefaultHasher::new()` uses fixed keys, so this is deterministic. A
    // 64-bit collision between two live tweens is not a practical concern.
    let mut h = std::hash::DefaultHasher::new();
    path_hash(from, &mut h);
    0xffu8.hash(&mut h);
    path_hash(to, &mut h);
    let key = h.finish();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(pos) = c.iter().position(|(k, _)| *k == key) {
            let entry = c.remove(pos);
            c.push(entry.clone());
            return entry.1;
        }
        let value = std::rc::Rc::new(align(from, to));
        if c.len() >= CACHE_SIZE {
            c.remove(0);
        }
        c.push((key, value.clone()));
        value
    })
}

impl Lerp for BezPath {
    fn lerp(&self, to: &Self, t: f64) -> Self {
        if t <= 0.0 {
            return self.clone();
        }
        if t >= 1.0 {
            return to.clone();
        }
        let pair = aligned(self, to);
        let mut out = BezPath::new();
        for (ra, rb) in pair.0.iter().zip(&pair.1) {
            let run = Run {
                segs: ra
                    .segs
                    .iter()
                    .zip(&rb.segs)
                    .map(|(x, y)| {
                        CubicBez::new(
                            x.p0.lerp(y.p0, t),
                            x.p1.lerp(y.p1, t),
                            x.p2.lerp(y.p2, t),
                            x.p3.lerp(y.p3, t),
                        )
                    })
                    .collect(),
                closed: ra.closed,
            };
            run.to_path(&mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use kurbo::{Circle, Rect, Shape};

    use super::*;

    fn sample(p: &BezPath, n: usize) -> Vec<Point> {
        let segs: Vec<_> = p.segments().collect();
        (0..n)
            .map(|i| {
                let u = i as f64 / n as f64 * segs.len() as f64;
                segs[(u as usize).min(segs.len() - 1)].eval(u.fract())
            })
            .collect()
    }

    #[test]
    fn endpoints_are_exact() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1);
        let b = Circle::new((5.0, 5.0), 5.0).to_path(0.1);
        assert_eq!(a.lerp(&b, 0.0), a);
        assert_eq!(a.lerp(&b, 1.0), b);
    }

    #[test]
    fn midpoint_of_identical_shapes_is_the_shape() {
        let a = Circle::new((0.0, 0.0), 10.0).to_path(0.1);
        let mid = a.lerp(&a, 0.5);
        for (p, q) in sample(&mid, 32).iter().zip(sample(&a, 32)) {
            assert!(p.distance(q) < 1e-9);
        }
    }

    #[test]
    fn rect_to_circle_stays_between_them() {
        let a = Rect::new(-10.0, -10.0, 10.0, 10.0).to_path(0.1);
        let b = Circle::new((0.0, 0.0), 10.0).to_path(0.1);
        let mid = a.lerp(&b, 0.5);
        // Every point lies between the inscribed circle and the square's
        // corner radius: no self-intersection, no collapse.
        for p in sample(&mid, 64) {
            let r = p.to_vec2().hypot();
            assert!((9.0..=14.2).contains(&r), "{p:?} r={r}");
        }
    }

    #[test]
    fn rotation_and_direction_are_aligned() {
        // Same square, different start corner and opposite direction:
        // the morph must not spin or pass through the center.
        let a = Rect::new(-10.0, -10.0, 10.0, 10.0).to_path(0.1);
        let mut b = BezPath::new();
        b.move_to((10.0, 10.0));
        b.line_to((10.0, -10.0));
        b.line_to((-10.0, -10.0));
        b.line_to((-10.0, 10.0));
        b.close_path();
        let mid = a.lerp(&b, 0.5);
        for p in sample(&mid, 16) {
            assert!(p.x.abs().max(p.y.abs()) > 9.99, "{p:?}");
        }
    }

    #[test]
    fn different_run_counts_grow_from_points() {
        let a = Circle::new((0.0, 0.0), 10.0).to_path(0.1);
        let mut b = Circle::new((-20.0, 0.0), 5.0).to_path(0.1);
        b.extend(Circle::new((20.0, 0.0), 5.0).to_path(0.1));
        let mid = a.lerp(&b, 0.5);
        assert_eq!(
            mid.elements()
                .iter()
                .filter(|e| matches!(e, PathEl::MoveTo(_)))
                .count(),
            2
        );
    }
}
