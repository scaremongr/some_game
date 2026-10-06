//! "Human-like player vs the sparring bot": what a player feels, in numbers.
//!   cargo run --release --example probe --manifest-path combat/Cargo.toml [matches] [style]
//! The player sees the opponent ~200 ms late, walks in, presses strings at a
//! human rhythm (a press every 8-12 ticks), blocks about half of what it sees,
//! throws a guard now and then and backs off when hurt. Reported: rounds won,
//! round length, hits and blocks, presses that did nothing (and why), stretches
//! without control, and time spent far apart. Not a proof of fun (see
//! docs/COMBAT_RESEARCH.md §14); a before/after yardstick for rule changes.
use arena_combat::moves::Height;
use arena_combat::*;

const SEE: usize = 12;
/// The rules have the quick rise and the uppercut chase (iteration 3).
const QUICK: bool = true;

struct Player {
    rng: u32,
    /// Remaining scripted presses: (ticks to wait, input bits, hold ticks).
    script: Vec<(u32, u32, u32)>,
    wait: u32,
    hold: u32,
    held: u32,
    retreat: u32,
    guard: u32,
}

impl Player {
    fn roll(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng % 100
    }
    fn input(&mut self, seen: &Match, side: usize) -> u32 {
        let me = &seen.fighters[side];
        let enemy = &seen.fighters[1 - side];
        let toward = if me.facing > 0 { RIGHT } else { LEFT };
        let away = if me.facing > 0 { LEFT } else { RIGHT };
        let distance = (enemy.x - me.x).abs();
        if self.hold > 0 {
            self.hold -= 1;
            return self.held;
        }
        if me.held > 0 {
            // Tries the tech about a third of the time.
            return if self.roll() < 3 { THROW } else { 0 };
        }
        // On the floor: a quick rise about half the time (when the new rules have one).
        if me.down > 0 {
            return if QUICK && me.frame == 24 && self.roll() < 50 { JUMP } else { 0 };
        }
        // Its uppercut landed: chases it into the air, usually.
        if QUICK && me.action == 10 && me.confirmed && self.script.is_empty() && self.roll() < 70 {
            self.script = vec![(6, JUMP | toward, 3), (4, KICK, 3)];
        }
        if self.guard > 0 {
            self.guard -= 1;
            return BLOCK | if enemy.attack().is_some_and(|m| m.height == Height::Low) { CROUCH } else { 0 };
        }
        // Sees an attack coming (late): blocks about half of them.
        if let Some(m) = enemy.attack() {
            if enemy.frame < m.startup && m.height != Height::Grab && self.script.is_empty() && self.roll() < 12 {
                self.guard = 14;
                return BLOCK;
            }
        }
        if self.wait > 0 {
            self.wait -= 1;
            return if self.retreat > 0 { away } else { 0 };
        }
        if let Some((wait, bits, hold)) = (!self.script.is_empty()).then(|| self.script.remove(0)) {
            self.wait = wait;
            self.hold = hold;
            self.held = bits;
            return bits;
        }
        if self.retreat > 0 {
            self.retreat -= 1;
            return away;
        }
        if me.hp < enemy.hp - 20 && self.roll() < 2 {
            self.retreat = 30;
        }
        if distance > 1350 {
            // A dash in now and then, otherwise walk.
            if distance > 2200 && self.roll() < 3 {
                self.wait = 20;
                return DASH | toward;
            }
            return if distance > 2600 { toward | RUN } else { toward };
        }
        let r = self.roll();
        let gap = |p: &mut Player| 8 + p.roll() % 5;
        self.script = match r {
            0..=29 => vec![(gap(self), LIGHT, 3), (gap(self), LIGHT, 3), (gap(self), LIGHT, 3)],
            30..=44 => vec![(gap(self), KICK, 3), (gap(self), KICK, 3)],
            45..=54 => vec![(gap(self), LIGHT, 3), (gap(self), LIGHT, 3), (gap(self), KICK, 3)],
            55..=64 => vec![(gap(self), CROUCH | LIGHT, 5), (gap(self), CROUCH | HEAVY, 5)],
            65..=71 => vec![(30, HEAVY, 3)],
            72..=79 => vec![(20, THROW, 3)],
            80..=86 => vec![(30, CROUCH | KICK, 4)],
            87..=93 => {
                self.guard = 40;
                vec![]
            }
            _ => {
                self.retreat = 20;
                vec![]
            }
        };
        0
    }
}

#[derive(Default)]
struct Stats {
    rounds: [u32; 2],
    round_ticks: Vec<u32>,
    hits: [u32; 2],
    blocks: [u32; 2],
    parries: [u32; 2],
    guard_breaks: [u32; 2],
    throws: [u32; 2],
    counters: [u32; 2],
    crumples: [u32; 2],
    air_chases: [u32; 2],
    knockdowns: [u32; 2],
    presses: u32,
    dead: u32,
    dead_stamina: u32,
    dead_why: [u32; 4],
    no_control: Vec<u32>,
    far: u64,
    ticks: u64,
    combo_hits: Vec<u32>,
}

fn controllable(f: &Fighter) -> bool {
    f.stun == 0 && f.down == 0 && f.held == 0 && f.blockstun == 0 && !(f.juggle > 0 && f.y > 0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(150);
    let style: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut s = Stats::default();
    for n in 0..matches {
        let mut m = Match::new(1000 + n);
        m.set_style(0, style);
        m.set_style(1, n % 3);
        let mut p = Player { rng: 0x9E37_79B9 ^ (n + 1).wrapping_mul(2_654_435_761), script: vec![], wait: 0, hold: 0, held: 0, retreat: 0, guard: 0 };
        let mut history: Vec<Match> = vec![];
        let mut round_start = 0;
        let mut lost = 0;
        let mut last_event = 0;
        let mut pending: Vec<(u32, u32)> = vec![];
        let mut combo = 0;
        while m.phase != 3 && m.tick < 60 * 60 * 6 {
            history.push(m.clone());
            if history.len() > SEE + 1 {
                history.remove(0);
            }
            let seen = &history[0];
            let input = if m.phase == 1 { p.input(seen, 0) } else { 0 };
            let bot = m.bot_input(1);
            let before = m.fighters[0].clone();
            let pressed = input & !before.previous & (LIGHT | HEAVY | KICK | THROW | SPECIAL);
            let phase = m.phase;
            m.step([input, bot]);
            if phase != 1 {
                round_start = m.tick;
                continue;
            }
            let f = &m.fighters[0];
            // A press counts as acted on if an attack starts within the buffer.
            if pressed != 0 {
                s.presses += 1;
                // Why it might do nothing: 0 free, 1 hurt, 2 busy with a move, 3 in the air.
                let why = if !controllable(&before) {
                    1
                } else if before.y > 0 {
                    3
                } else if before.action != 0 {
                    2
                } else {
                    0
                };
                pending.push((m.tick + 12, why));
            }
            if f.frame == 0 && f.action != 0 && f.action != 5 && f.action != 15 && f.action != 3 {
                pending.clear();
            }
            pending.retain(|&(until, why)| {
                if m.tick > until {
                    s.dead += 1;
                    s.dead_why[why as usize] += 1;
                    if why == 0 && f.stamina < 220 {
                        s.dead_stamina += 1;
                    }
                    false
                } else {
                    true
                }
            });
            if controllable(&m.fighters[0]) || (m.freeze > 0 && lost == 0) {
                if lost > 0 {
                    s.no_control.push(lost);
                }
                lost = 0;
            } else {
                lost += 1;
            }
            s.ticks += 1;
            if (m.fighters[0].x - m.fighters[1].x).abs() > 2200 {
                s.far += 1;
            }
            if m.event != last_event {
                last_event = m.event;
                let attacker = 1 - m.event_target;
                match m.event_kind {
                    1 | 6 | 7 | 11 => {
                        s.hits[attacker] += 1;
                        if m.event_kind == 6 {
                            s.counters[attacker] += 1;
                        }
                        if m.event_kind == 11 {
                            s.crumples[attacker] += 1;
                        }
                        if m.fighters[attacker].y > 0 && m.fighters[1 - attacker].juggle >= 2 {
                            s.air_chases[attacker] += 1;
                        }
                        if attacker == 0 {
                            combo += 1;
                        }
                    }
                    2 => s.blocks[m.event_target] += 1,
                    3 => s.parries[m.event_target] += 1,
                    4 => s.guard_breaks[attacker] += 1,
                    10 => s.throws[attacker] += 1,
                    _ => {}
                }
                if m.fighters[m.event_target].down == KNOCKDOWN {
                    s.knockdowns[attacker] += 1;
                }
            }
            if m.fighters[0].combo == 0 && combo > 0 {
                s.combo_hits.push(combo);
                combo = 0;
            }
            if m.phase != 1 {
                s.round_ticks.push(m.tick - round_start);
                if m.winner >= 0 {
                    s.rounds[m.winner as usize] += 1;
                }
            }
        }
    }
    let pct = |v: &mut Vec<u32>, q: f64| {
        v.sort();
        if v.is_empty() { 0 } else { v[((v.len() - 1) as f64 * q) as usize] }
    };
    let total = (s.rounds[0] + s.rounds[1]).max(1);
    println!("matches {matches}, player style {style}");
    println!("rounds won by player   {:.0}% ({} of {})", 100.0 * s.rounds[0] as f64 / total as f64, s.rounds[0], total);
    let mut rt = s.round_ticks.clone();
    let mean = rt.iter().map(|&t| t as f64).sum::<f64>() / rt.len().max(1) as f64 / 60.0;
    println!("round length           mean {:.1} s, p90 {:.1} s", mean, pct(&mut rt, 0.9) as f64 / 60.0);
    println!("hits player/bot        {} / {}", s.hits[0], s.hits[1]);
    println!("blocks by player/bot   {} / {}", s.blocks[0], s.blocks[1]);
    println!("parries player/bot     {} / {}", s.parries[0], s.parries[1]);
    println!("guard breaks by p/b    {} / {}", s.guard_breaks[0], s.guard_breaks[1]);
    println!("throws by p/b          {} / {}", s.throws[0], s.throws[1]);
    println!("counters by p/b        {} / {} (crumples {} / {})", s.counters[0], s.counters[1], s.crumples[0], s.crumples[1]);
    println!("air chases by p/b      {} / {}", s.air_chases[0], s.air_chases[1]);
    println!("knockdowns by p/b      {} / {}", s.knockdowns[0], s.knockdowns[1]);
    println!(
        "player presses         {} — did nothing {:.1}% (low stamina {:.1}%)",
        s.presses,
        100.0 * s.dead as f64 / s.presses.max(1) as f64,
        100.0 * s.dead_stamina as f64 / s.presses.max(1) as f64
    );
    let share = |v: u32| 100.0 * v as f64 / s.presses.max(1) as f64;
    println!(
        "  of them: free {:.1}%, hurt {:.1}%, busy {:.1}%, airborne {:.1}%",
        share(s.dead_why[0]),
        share(s.dead_why[1]),
        share(s.dead_why[2]),
        share(s.dead_why[3])
    );
    let mut nc = s.no_control.clone();
    println!(
        "player without control p50 {:.2} s, p95 {:.2} s, max {:.2} s ({} stretches)",
        pct(&mut nc, 0.5) as f64 / 60.0,
        pct(&mut nc, 0.95) as f64 / 60.0,
        nc.last().copied().unwrap_or(0) as f64 / 60.0,
        nc.len()
    );
    let mut ch = s.combo_hits.clone();
    println!("player combos (hits)   p50 {}, p90 {}, max {}", pct(&mut ch, 0.5), pct(&mut ch, 0.9), ch.last().copied().unwrap_or(0));
    println!("far apart (> 2.2 m)    {:.0}% of fighting time", 100.0 * s.far as f64 / s.ticks.max(1) as f64);
}
