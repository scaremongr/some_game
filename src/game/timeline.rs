//! Render-time sampling of authoritative fight snapshots.
//!
//! The simulation advances in whole 60 Hz ticks (every tick locally, every
//! third tick over the network), while the screen refreshes at 60-120 Hz with
//! its own jitter. Drawing the latest snapshot as-is shows repeated and skipped
//! frames. Instead the scene renders at a continuous tick: positions are
//! interpolated between the two surrounding snapshots and every animation
//! plays on a fractional frame counter, so motion is smooth at any refresh
//! rate. Past the newest snapshot a few ticks are extrapolated.
use arena_combat::{Fighter, Match};
use std::collections::VecDeque;

const HISTORY: usize = 16;
const MAX_EXTRAPOLATION: f64 = 4.0;

/// Continuous kinematics of one fighter at the render tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    /// Metres.
    pub x: f32,
    pub y: f32,
    /// Vertical speed, mm per tick (as in the simulation).
    pub vy: f32,
    /// Fractional frame of the current action.
    pub frame: f32,
}

pub struct Sample {
    pub fighters: [Fighter; 2],
    pub bodies: [Body; 2],
    /// Index of the snapshot whose discrete state is shown.
    pub index: usize,
}

#[derive(Default)]
pub struct Timeline {
    states: VecDeque<Match>,
}

impl Timeline {
    /// Adds a snapshot. Returns true when the fight restarted (new match or round).
    pub fn push(&mut self, state: Match) -> bool {
        let reset = self
            .states
            .back()
            .is_some_and(|last| state.tick < last.tick || state.round != last.round);
        if reset {
            self.states.clear();
        }
        match self.states.back_mut() {
            Some(last) if last.tick == state.tick => *last = state,
            _ => self.states.push_back(state),
        }
        while self.states.len() > HISTORY {
            self.states.pop_front();
        }
        reset
    }
    pub fn latest(&self) -> Option<&Match> {
        self.states.back()
    }
    pub fn get(&self, index: usize) -> Option<&Match> {
        self.states.get(index)
    }

    /// State at render tick `r`; NaN renders the newest snapshot exactly.
    pub fn sample(&self, r: f64) -> Option<Sample> {
        let last = self.states.len().checked_sub(1)?;
        let newest = &self.states[last];
        if !r.is_finite() || last == 0 {
            let exact = |f: &Fighter| Body {
                x: f.x as f32 / 1000.0,
                y: f.y as f32 / 1000.0,
                vy: f.vy as f32,
                frame: f.frame as f32,
            };
            let [a, b] = &newest.fighters;
            return Some(Sample {
                fighters: newest.fighters.clone(),
                bodies: [exact(a), exact(b)],
                index: last,
            });
        }
        let r = r.clamp(self.states[0].tick as f64, newest.tick as f64 + MAX_EXTRAPOLATION);
        let i = self
            .states
            .iter()
            .rposition(|s| s.tick as f64 <= r)
            .unwrap_or(0);
        if i == last {
            let previous = &self.states[last - 1];
            let fighters = newest.fighters.clone();
            let bodies = std::array::from_fn(|s| {
                extrapolate(&previous.fighters[s], previous.tick, &newest.fighters[s], newest.tick, newest.freeze, r)
            });
            return Some(Sample { fighters, bodies, index: last });
        }
        let (a, b) = (&self.states[i], &self.states[i + 1]);
        let mut index = i;
        let mut fighters = a.fighters.clone();
        let mut bodies = [Body::default(); 2];
        for s in 0..2 {
            let (fa, fb) = (&a.fighters[s], &b.fighters[s]);
            let span = (b.tick - a.tick) as f64;
            let u = ((r - a.tick as f64) / span).clamp(0.0, 1.0) as f32;
            let lerp = |p: i32, q: i32| {
                if (q - p).abs() > 1500 {
                    (if u < 0.5 { p } else { q }) as f32 / 1000.0
                } else {
                    (p as f32 + (q - p) as f32 * u) / 1000.0
                }
            };
            let continuing = fa.action == fb.action
                && fb.frame >= fa.frame
                && (fb.frame - fa.frame) as f64 <= span + 0.5;
            let frame = if continuing {
                fa.frame as f32 + (fb.frame - fa.frame) as f32 * u
            } else {
                // The new action began `fb.frame` ticks before snapshot b.
                let start = b.tick as f64 - fb.frame as f64;
                if r >= start {
                    fighters[s] = fb.clone();
                    index = i + 1;
                    (fb.frame as f64 - (b.tick as f64 - r)).max(0.0) as f32
                } else {
                    fa.frame as f32 + (r - a.tick as f64) as f32
                }
            };
            bodies[s] = Body {
                x: lerp(fa.x, fb.x),
                y: lerp(fa.y, fb.y).max(0.0),
                vy: fa.vy as f32 + (fb.vy - fa.vy) as f32 * u,
                frame,
            };
        }
        Some(Sample { fighters, bodies, index })
    }
}

fn extrapolate(p: &Fighter, pt: u32, f: &Fighter, t: u32, freeze: u32, r: f64) -> Body {
    let d = (r - t as f64).clamp(0.0, MAX_EXTRAPOLATION) as f32;
    let dt = (t - pt).max(1) as f32;
    let vx = if (f.x - p.x).abs() > 1500 { 0.0 } else { (f.x - p.x) as f32 / dt };
    let y = if f.y > 0 {
        (f.y as f32 + f.vy as f32 * d - 2.5 * d * d).max(0.0)
    } else {
        0.0
    };
    let moving = freeze == 0 && f.action != 0;
    Body {
        x: (f.x as f32 + vx * d) / 1000.0,
        y: y / 1000.0,
        vy: f.vy as f32 - if f.y > 0 { 5.0 * d } else { 0.0 },
        frame: f.frame as f32 + if moving { d.min(3.0) } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_combat::*;

    #[test]
    fn interpolates_positions_and_frames_between_network_snapshots() {
        let mut m = Match::new(3);
        m.phase = 1;
        m.fighters[0].x = -600;
        m.fighters[1].x = 600;
        let mut timeline = Timeline::default();
        let mut inputs = [LIGHT | RIGHT, 0];
        for tick in 0..30 {
            m.step(inputs);
            inputs = [RIGHT, 0];
            if tick % 3 == 0 {
                timeline.push(m.clone());
            }
        }
        let first = timeline.get(0).unwrap().tick as f64;
        let newest = timeline.latest().unwrap().tick as f64;
        let mut previous: Option<Sample> = None;
        let mut r = first;
        while r <= newest {
            let s = timeline.sample(r).unwrap();
            if let Some(p) = &previous {
                // No step backwards and no jump larger than one tick of motion.
                assert!(s.bodies[0].x >= p.bodies[0].x - 1e-4);
                assert!(s.bodies[0].x - p.bodies[0].x < 0.08);
                if s.fighters[0].action == p.fighters[0].action && s.fighters[0].action != 0 {
                    assert!(s.bodies[0].frame >= p.bodies[0].frame - 1e-3);
                }
            }
            previous = Some(s);
            r += 0.25;
        }
    }

    #[test]
    fn restart_clears_history_and_extrapolation_is_bounded() {
        let mut timeline = Timeline::default();
        let mut m = Match::new(1);
        m.tick = 50;
        timeline.push(m.clone());
        m.tick = 51;
        m.fighters[0].x = -1100;
        timeline.push(m.clone());
        let far = timeline.sample(1000.0).unwrap();
        assert!((far.bodies[0].x - (-1.1 + 0.05 * 4.0)).abs() < 1e-3);
        let mut fresh = Match::new(2);
        fresh.tick = 3;
        assert!(timeline.push(fresh));
        assert_eq!(timeline.latest().unwrap().tick, 3);
    }
}
