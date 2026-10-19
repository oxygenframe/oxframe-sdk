use kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathSeg};

const ACCURACY: f64 = 1e-3;

/// The part of `path` between fractions `start` and `end` of its total
/// length (`0 ≤ start ≤ end ≤ 1`), as used by the `start`/`end` signals.
pub fn trim(path: &BezPath, start: f64, end: f64) -> BezPath {
    let (start, end) = (start.clamp(0.0, 1.0), end.clamp(0.0, 1.0));
    if start <= 0.0 && end >= 1.0 {
        return path.clone();
    }
    let mut out = BezPath::new();
    if end <= start {
        return out;
    }
    let segs: Vec<(PathSeg, f64)> = path.segments().map(|s| (s, s.arclen(ACCURACY))).collect();
    let total: f64 = segs.iter().map(|(_, l)| l).sum();
    if total <= 0.0 {
        return out;
    }
    let (a, b) = (start * total, end * total);
    let mut acc = 0.0;
    let mut last_end: Option<kurbo::Point> = None;
    for (seg, len) in segs {
        let (s0, s1) = (acc, acc + len);
        acc = s1;
        if s1 <= a || s0 >= b || len <= 0.0 {
            continue;
        }
        let t0 = if a > s0 {
            seg.inv_arclen(a - s0, ACCURACY)
        } else {
            0.0
        };
        let t1 = if b < s1 {
            seg.inv_arclen(b - s0, ACCURACY)
        } else {
            1.0
        };
        let piece = seg.subsegment(t0..t1);
        let p0 = piece.start();
        if last_end.is_none_or(|e| e.distance(p0) > 1e-6) {
            out.move_to(p0);
        }
        match piece {
            PathSeg::Line(l) => out.line_to(l.p1),
            PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
        }
        last_end = Some(piece.end());
    }
    out
}

#[cfg(test)]
mod tests {
    use kurbo::{Line, Shape};

    use super::*;

    #[test]
    fn trims_by_length() {
        let p = Line::new((0.0, 0.0), (100.0, 0.0)).to_path(0.1);
        let t = trim(&p, 0.25, 0.5);
        let bb = t.bounding_box();
        assert!((bb.x0 - 25.0).abs() < 0.01 && (bb.x1 - 50.0).abs() < 0.01);
        assert!(trim(&p, 0.5, 0.5).elements().is_empty());
        assert_eq!(trim(&p, 0.0, 1.0), p);
    }
}
