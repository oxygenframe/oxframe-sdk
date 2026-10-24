//! Where things move: for each node that travels, the stretches of time it
//! moves (its keyframes, as `check` sees them) and the path of its center.
//! `check` lists them in `report.json` and draws them per scene.

use serde::Serialize;

use crate::lint::Sample;

/// Per node: its parent, name, `add` call site, and center at each sample.
type Paths = std::collections::BTreeMap<u32, (Option<u32>, String, Option<String>, Vec<[f64; 3]>)>;

/// A node moves when its center travels at least this far, in scene units.
const MIN_TRAVEL: f64 = 24.0;
/// Between two samples (0.1 s apart by default), a smaller step is standing
/// still: slower than about 5 px/s is not visible motion.
const STILL: f64 = 0.5;
/// At most this many paths, the longest ones.
const MAX_PATHS: usize = 12;

/// One node that travels.
#[derive(Debug, Clone, Serialize)]
pub struct Motion {
    /// `.name(..)`, or the `add` call site.
    pub node: String,
    /// `file:line` of the `add` call, when known.
    pub source: Option<String>,
    /// Stretches of time it moves, `[start, end]` in seconds.
    pub moves: Vec<[f64; 2]>,
    /// Center where it starts and where it ends, in scene units.
    pub from: [f64; 2],
    /// See [`Motion::from`].
    pub to: [f64; 2],
    /// Length of the path, in scene units.
    pub distance: f64,
    /// Center at each sample, `[time, x, y]` (not in the JSON).
    #[serde(skip)]
    pub path: Vec<[f64; 3]>,
    #[serde(skip)]
    id: u32,
}

/// The nodes that travel, the 12 with the longest paths. A node
/// that only rides along with a moving parent is left out: its parent's
/// path says it.
pub fn motions(samples: &[Sample], step: f64) -> Vec<Motion> {
    let mut paths = Paths::new();
    for s in samples {
        for n in &s.nodes {
            if n.id == 0 || n.opacity <= 0.05 || !n.bounds.iter().all(|v| v.is_finite()) {
                continue;
            }
            let c = [
                (n.bounds[0] + n.bounds[2]) / 2.0,
                (n.bounds[1] + n.bounds[3]) / 2.0,
            ];
            let entry = paths
                .entry(n.id)
                .or_insert_with(|| (n.parent, n.name.clone(), n.source.clone(), Vec::new()));
            entry.3.push([s.time, c[0], c[1]]);
        }
    }
    let mut found: Vec<Motion> = Vec::new();
    for (id, (_, node, source, path)) in &paths {
        let Some(first) = path.first() else { continue };
        let travel = path
            .iter()
            .map(|p| (p[1] - first[1]).hypot(p[2] - first[2]))
            .fold(0.0, f64::max);
        if travel < MIN_TRAVEL {
            continue;
        }
        let mut moves: Vec<[f64; 2]> = Vec::new();
        let mut distance = 0.0;
        for w in path.windows(2) {
            let d = (w[1][1] - w[0][1]).hypot(w[1][2] - w[0][2]);
            if d <= STILL {
                continue;
            }
            distance += d;
            match moves.last_mut() {
                Some(last) if w[0][0] - last[1] <= step * 1.5 => last[1] = w[1][0],
                _ => moves.push([w[0][0], w[1][0]]),
            }
        }
        let last = path[path.len() - 1];
        found.push(Motion {
            node: node.clone(),
            source: source.clone(),
            moves,
            from: [first[1], first[2]],
            to: [last[1], last[2]],
            distance,
            path: path.clone(),
            id: *id,
        });
    }
    // A child whose every step matches its parent's is carried, not moved.
    let carried: Vec<u32> = found
        .iter()
        .filter(|m| {
            let Some(parent) = paths.get(&m.id).and_then(|p| p.0) else {
                return false;
            };
            let Some(pm) = found.iter().find(|p| p.id == parent) else {
                return false;
            };
            same_steps(&m.path, &pm.path)
        })
        .map(|m| m.id)
        .collect();
    found.retain(|m| !carried.contains(&m.id));
    found.sort_by(|a, b| b.distance.total_cmp(&a.distance));
    found.truncate(MAX_PATHS);
    found.sort_by(|a, b| {
        let start = |m: &Motion| m.moves.first().map_or(f64::INFINITY, |s| s[0]);
        start(a).total_cmp(&start(b))
    });
    found
}

/// Both paths take the same steps at the same times, within 2 units.
fn same_steps(child: &[[f64; 3]], parent: &[[f64; 3]]) -> bool {
    let steps = |p: &[[f64; 3]]| -> Vec<[f64; 3]> {
        p.windows(2)
            .map(|w| [w[1][0], w[1][1] - w[0][1], w[1][2] - w[0][2]])
            .collect()
    };
    let (c, p) = (steps(child), steps(parent));
    c.iter().all(|cs| {
        p.iter()
            .find(|ps| (ps[0] - cs[0]).abs() < 1e-6)
            .is_some_and(|ps| (ps[1] - cs[1]).abs() < 2.0 && (ps[2] - cs[2]).abs() < 2.0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxframe_2d::InspectNode;
    use oxframe_core::Color;

    fn node(id: u32, parent: u32, x: f64) -> InspectNode {
        InspectNode {
            id,
            parent: Some(parent),
            kind: "Rect",
            name: format!("n{id}"),
            source: Some(format!("src/main.rs:{id}")),
            bounds: [x - 10.0, -10.0, x + 10.0, 10.0],
            opacity: 1.0,
            order: id,
            text: None,
            fill: Color::WHITE,
            text_overflow: 0.0,
            parent_kind: Some("Group"),
            in_camera: false,
            shader: None,
            params: Vec::new(),
        }
    }

    fn samples(xs: &[(f64, f64, f64)]) -> Vec<Sample> {
        // (time, x of node 1, x of node 3); node 2 rides on node 1.
        xs.iter()
            .map(|&(t, a, b)| Sample {
                time: t,
                nodes: vec![node(1, 0, a), node(2, 1, a + 50.0), node(3, 0, b)],
                contrast: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn finds_the_stretches_a_node_moves_and_drops_riders() {
        let s = samples(&[
            (0.0, 0.0, 0.0),
            (0.1, 0.0, 0.0),
            (0.2, 50.0, 0.0),
            (0.3, 100.0, 0.0),
            (0.4, 100.0, 0.0),
            (0.5, 100.0, 0.0),
            (0.6, 100.0, 10.0),
        ]);
        let found = motions(&s, 0.1);
        // Node 3 travels 10 units: standing still. Node 2 rides on node 1.
        assert_eq!(found.len(), 1, "{found:?}");
        let m = &found[0];
        assert_eq!(m.node, "n1");
        assert_eq!(m.moves, vec![[0.1, 0.3]]);
        assert_eq!(m.from, [0.0, 0.0]);
        assert_eq!(m.to, [100.0, 0.0]);
        assert!((m.distance - 100.0).abs() < 1e-9);
    }
}
