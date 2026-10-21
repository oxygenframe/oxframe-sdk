//! Deterministic particle simulation. State at time `t` is computed from the
//! bursts and a fixed step, from the latest checkpoint at or before `t`, so
//! seeking backwards replays at most a bounded number of steps.

use std::cell::RefCell;

use oxframe_core::{SeededRng, Vec2};

/// One emission, at a script time.
#[derive(Clone, Copy)]
pub(crate) struct Burst {
    pub time: f64,
    pub origin: Vec2,
    pub count: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct Particle {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub age: f64,
    pub life: f64,
    pub radius: f64,
}

#[derive(Clone, Copy)]
pub(crate) struct SimParams {
    pub seed: u64,
    pub gravity: f64,
    pub toward_camera: bool,
    pub life: f64,
    pub speed: f64,
    pub radius: f64,
}

#[derive(Clone)]
struct Checkpoint {
    t: f64,
    applied: usize,
    steps: u32,
    particles: Vec<Particle>,
}

/// Integrated up to [`Self::t`]. Checkpoints are every [`CHECKPOINT_STEPS`]
/// fixed steps.
pub(crate) struct ParticleSim {
    t: f64,
    applied: usize,
    steps: u32,
    particles: Vec<Particle>,
    checkpoints: Vec<Checkpoint>,
}

const STEP: f64 = 1.0 / 120.0;
const CHECKPOINT_STEPS: u32 = 30;

impl ParticleSim {
    pub(crate) fn new() -> Self {
        Self {
            t: 0.0,
            applied: 0,
            steps: 0,
            particles: Vec::new(),
            checkpoints: vec![Checkpoint {
                t: 0.0,
                applied: 0,
                steps: 0,
                particles: Vec::new(),
            }],
        }
    }

    pub(crate) fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// Brings the simulation to `target`. Going backwards restores the latest
    /// checkpoint at or before `target` and steps forward from there.
    pub(crate) fn at(&mut self, target: f64, bursts: &[Burst], params: SimParams) {
        let target = target.max(0.0);
        if target + 1e-9 < self.t {
            let i = self
                .checkpoints
                .iter()
                .rposition(|c| c.t <= target + 1e-9)
                .unwrap_or(0);
            let cp = self.checkpoints[i].clone();
            self.checkpoints.truncate(i + 1);
            self.t = cp.t;
            self.applied = cp.applied;
            self.steps = cp.steps;
            self.particles = cp.particles;
        }
        while self.t + 1e-12 < target {
            let dt = STEP.min(target - self.t);
            let t1 = self.t + dt;
            while self.applied < bursts.len() && bursts[self.applied].time <= t1 + 1e-12 {
                self.spawn(&bursts[self.applied], self.applied, params);
                self.applied += 1;
            }
            self.kick(dt, params.gravity);
            self.t = t1;
            self.steps += 1;
            if self.steps.is_multiple_of(CHECKPOINT_STEPS) {
                self.checkpoints.push(Checkpoint {
                    t: self.t,
                    applied: self.applied,
                    steps: self.steps,
                    particles: self.particles.clone(),
                });
            }
        }
    }

    fn spawn(&mut self, burst: &Burst, index: usize, params: SimParams) {
        let mut rng = SeededRng::new(
            params
                .seed
                .wrapping_add((index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
        );
        for _ in 0..burst.count {
            let ang = rng.range(0.0..std::f64::consts::TAU);
            let spd = rng.range(params.speed * 0.45..params.speed);
            self.particles.push(Particle {
                x: burst.origin.x,
                y: burst.origin.y,
                z: 1.0,
                vx: ang.cos() * spd,
                // Negative y is up.
                vy: -spd * 0.35 + ang.sin() * spd * 0.55,
                vz: if params.toward_camera {
                    rng.range(-1.6..-0.5)
                } else {
                    0.0
                },
                age: 0.0,
                life: params.life.max(STEP),
                radius: params.radius * rng.range(0.6..1.3),
            });
        }
    }

    fn kick(&mut self, dt: f64, gravity: f64) {
        for p in &mut self.particles {
            p.vy += gravity * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.z = (p.z + p.vz * dt).max(0.08);
            p.age += dt;
        }
        self.particles.retain(|p| p.age < p.life);
    }
}

impl Default for ParticleSim {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal particle state.
#[doc(hidden)]
pub struct ParticlesState {
    pub(crate) bursts: RefCell<Vec<Burst>>,
    pub(crate) sim: RefCell<ParticleSim>,
}

impl ParticlesState {
    pub(crate) fn new() -> Self {
        Self {
            bursts: RefCell::new(Vec::new()),
            sim: RefCell::new(ParticleSim::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> SimParams {
        SimParams {
            seed: 7,
            gravity: 800.0,
            toward_camera: true,
            life: 2.0,
            speed: 200.0,
            radius: 4.0,
        }
    }

    #[test]
    fn a_later_time_rewinds_to_the_same_particles() {
        let bursts = [Burst {
            time: 0.0,
            origin: Vec2::new(0.0, 0.0),
            count: 20,
        }];
        let mut sim = ParticleSim::new();
        sim.at(0.4, &bursts, params());
        let mid: Vec<_> = sim
            .particles()
            .iter()
            .map(|p| (p.x, p.y, p.z, p.age))
            .collect();
        sim.at(1.2, &bursts, params());
        sim.at(0.4, &bursts, params());
        let again: Vec<_> = sim
            .particles()
            .iter()
            .map(|p| (p.x, p.y, p.z, p.age))
            .collect();
        assert_eq!(mid.len(), 20);
        assert_eq!(mid, again);
    }

    #[test]
    fn the_same_bursts_always_integrate_the_same_way() {
        let bursts = [Burst {
            time: 0.1,
            origin: Vec2::new(3.0, -2.0),
            count: 8,
        }];
        let mut a = ParticleSim::new();
        let mut b = ParticleSim::new();
        a.at(0.8, &bursts, params());
        b.at(0.8, &bursts, params());
        let sig = |s: &ParticleSim| {
            s.particles()
                .iter()
                .map(|p| (p.x.to_bits(), p.y.to_bits(), p.z.to_bits()))
                .collect::<Vec<_>>()
        };
        assert_eq!(sig(&a), sig(&b));
    }
}
