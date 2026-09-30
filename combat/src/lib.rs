//! Authoritative, fixed 60 Hz combat. No clock, graphics, network or random IO.
use nanoserde::{DeJson, SerJson};
pub mod moves;
pub mod room;
use moves::{attack, Height};

pub const LEFT: u32 = 1;
pub const RIGHT: u32 = 2;
pub const BLOCK: u32 = 4;
pub const LIGHT: u32 = 8;
pub const HEAVY: u32 = 16;
pub const DASH: u32 = 32;
pub const THROW: u32 = 64;
pub const KICK: u32 = 128;
pub const CROUCH: u32 = 256;
pub const JUMP: u32 = 512;
pub const SPECIAL: u32 = 1024;
pub const SMASH: u32 = 2048;
pub const INPUT_MASK: u32 = 4095;
pub const EDGE_MASK: u32 = LIGHT | HEAVY | DASH | THROW | KICK | JUMP | SPECIAL | SMASH;
/// Ticks on the floor after a knockdown, including the visible wake-up.
pub const KNOCKDOWN: u32 = 72;
use room::ARENA_LIMIT;
// action: 0 idle, 1 jab, 2 heavy, 3 dash, 4 throw, 5 stun
#[derive(Clone, Debug, SerJson, DeJson)]
pub struct Fighter {
    pub x: i32,
    /// Integer millimetres/tick; authoritative impulse, gravity and body spring.
    pub vx: i32,
    pub y: i32,
    pub vy: i32,
    pub recoil: i32,
    pub recoil_v: i32,
    pub wall_cooldown: u32,
    #[nserde(default)]
    pub room_cooldown: u32,
    pub hp: i32,
    pub stamina: i32,
    pub action: u32,
    pub frame: u32,
    pub facing: i32,
    pub guard: bool,
    pub guard_age: u32,
    pub stun: u32,
    pub combo: u32,
    pub combo_time: u32,
    pub connected: bool,
    pub dash_dir: i32,
    pub previous: u32,
    pub buffered: u32,
    pub buffer_time: u32,
    pub crouch: bool,
    pub meter: i32,
    pub blockstun: u32,
    pub down: u32,
    pub invulnerable: u32,
    pub juggle: u32,
    pub combo_damage: i32,
    pub confirmed: bool,
    pub prop_hit: bool,
    pub air_attack: bool,
    /// Ticks left in the thrower's grip: pinned in front of the attacker,
    /// lifted and thrown; on the floor (knocked down) when it reaches 0.
    #[nserde(default)]
    pub held: u32,
}
/// A throw holds its victim this long, from the grab to the slam.
pub const HOLD: u32 = 32;
/// Distance between the thrower and the held victim (mm).
const HOLD_DISTANCE: i32 = 600;
impl Fighter {
    fn new(x: i32, facing: i32) -> Self {
        Self {
            x,
            vx: 0,
            y: 0,
            vy: 0,
            recoil: 0,
            recoil_v: 0,
            wall_cooldown: 0,
            room_cooldown: 0,
            hp: 100,
            stamina: 1000,
            action: 0,
            frame: 0,
            facing,
            guard: false,
            guard_age: 0,
            stun: 0,
            combo: 0,
            combo_time: 0,
            connected: false,
            dash_dir: facing,
            previous: 0,
            buffered: 0,
            buffer_time: 0,
            crouch: false,
            meter: 400,
            blockstun: 0,
            down: 0,
            invulnerable: 0,
            juggle: 0,
            combo_damage: 0,
            confirmed: false,
            prop_hit: false,
            air_attack: false,
            held: 0,
        }
    }
}
#[derive(Clone, Debug, SerJson, DeJson)]
pub struct Wall {
    pub hp: i32,
    pub impacts: u32,
    pub broken_tick: u32,
    pub impulse: i32,
}
impl Default for Wall {
    fn default() -> Self {
        Self {
            hp: 110,
            impacts: 0,
            broken_tick: 0,
            impulse: 0,
        }
    }
}
#[derive(Clone, Debug, SerJson, DeJson)]
pub struct Match {
    pub tick: u32,
    pub fighters: [Fighter; 2],
    pub walls: [Wall; 2],
    pub objects: Vec<room::ObjectState>,
    pub score: [u32; 2],
    pub round: u32,
    pub remaining: u32,
    // 0 countdown, 1 fighting, 2 round result, 3 match result
    pub phase: u32,
    pub phase_ticks: u32,
    pub winner: i32,
    pub freeze: u32,
    pub event: u32,
    // 1 hit, 2 block, 3 parry, 4 guard break, 5 throw, 6 counter
    pub event_kind: u32,
    pub event_target: usize,
    pub seed: u32,
}
impl Default for Match {
    fn default() -> Self {
        Self::new(1)
    }
}
impl Match {
    pub fn new(seed: u32) -> Self {
        Self {
            tick: 0,
            fighters: [Fighter::new(-1150, 1), Fighter::new(1150, -1)],
            walls: [Wall::default(), Wall::default()],
            objects: room::fresh(),
            score: [0, 0],
            round: 1,
            remaining: 60 * 60,
            phase: 0,
            phase_ticks: 150,
            winner: -1,
            freeze: 0,
            event: 0,
            event_kind: 0,
            event_target: 0,
            seed: seed.max(1),
        }
    }
    pub fn forfeit(&mut self, loser: usize) {
        if loser < 2 && self.phase != 3 {
            self.winner = (1 - loser) as i32;
            self.phase = 3;
        }
    }
    fn random(&mut self) -> u32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed
    }
    /// Bot commits at human-scale intervals and sometimes misreads the opponent.
    pub fn bot_input(&mut self, side: usize) -> u32 {
        let distance = (self.fighters[0].x - self.fighters[1].x).abs();
        let toward = if side == 0 { RIGHT } else { LEFT };
        let away = if side == 0 { LEFT } else { RIGHT };
        let enemy = &self.fighters[1 - side];
        let me = &self.fighters[side];
        if me.stun > 0 && me.meter >= 1000 && self.tick % 17 == 0 {
            return BLOCK | DASH;
        }
        if me.confirmed {
            if me.action == 1 && me.frame >= 9 {
                return if me.previous & LIGHT == 0 { LIGHT } else { 0 };
            }
            if me.action == 11 && me.frame >= 8 {
                return KICK;
            }
        }
        if me.y > 200 && distance < 1500 {
            return KICK;
        }
        if attack(enemy.action).is_some_and(|m| enemy.frame > 6 && enemy.frame < m.startup)
            && self.tick % 5 != 0
        {
            return BLOCK | if enemy.action == 9 { CROUCH } else { 0 };
        }
        if distance > 1450 {
            return toward;
        }
        if me.stamina < 240 {
            return away | BLOCK;
        }
        if self.tick % 19 != 0 {
            return if distance < 700 { away } else { 0 };
        }
        let meter = me.meter;
        match self.random() % 12 {
            0 => away | DASH,
            1 if distance < 1000 => THROW,
            2 => CROUCH | KICK,
            3 => CROUCH | HEAVY,
            4 => JUMP | toward,
            5 if meter >= 500 => SPECIAL,
            6 => HEAVY,
            7..=8 => KICK,
            _ => LIGHT,
        }
    }
    /// Between rounds and after the match nobody fights, but bodies still
    /// fall and slide to rest: a fighter caught mid-jump by the final blow
    /// or the timer lands instead of hanging in the air. True while moving.
    fn settle(&mut self) -> bool {
        let bounds = [-ARENA_LIMIT, ARENA_LIMIT];
        let mut moving = false;
        for f in &mut self.fighters {
            if f.y == 0 && f.vx == 0 {
                continue;
            }
            moving = true;
            f.x = (f.x + f.vx).clamp(bounds[0], bounds[1]);
            f.vx = f.vx * if f.y > 0 { 97 } else { 87 } / 100;
            if f.vx.abs() < 2 {
                f.vx = 0;
            }
            f.y = (f.y + f.vy).max(0);
            if f.y > 0 {
                f.vy -= 5;
            } else {
                f.vy = 0;
            }
        }
        moving
    }
    pub fn step(&mut self, inputs: [u32; 2]) {
        if self.phase == 3 {
            if self.settle() {
                self.tick += 1;
            }
            return;
        }
        self.tick += 1;
        if self.phase != 1 {
            self.settle();
            self.phase_ticks = self.phase_ticks.saturating_sub(1);
            if self.phase_ticks == 0 {
                if self.phase == 2 {
                    self.round += 1;
                    let center = room::ROUND_CENTERS[(self.round as usize - 1) % room::ROUND_CENTERS.len()];
                    self.fighters = [Fighter::new(center - 1150, 1), Fighter::new(center + 1150, -1)];
                    self.walls = [Wall::default(), Wall::default()];
                    self.objects = room::fresh();
                    self.remaining = 3600;
                    self.phase = 0;
                    self.phase_ticks = 120;
                } else {
                    self.phase = 1;
                }
            }
            // Holding an attack through countdown does not inject free attacks.
            for (f, input) in self.fighters.iter_mut().zip(inputs) {
                f.previous = input;
            }
            return;
        }
        // Capture attack edges even during hitstop; short presses are never swallowed.
        for (f, input) in self.fighters.iter_mut().zip(inputs) {
            let input = input & INPUT_MASK;
            let pressed = input & !f.previous;
            f.previous = input;
            if pressed & EDGE_MASK != 0 {
                f.buffered = (pressed & EDGE_MASK) | (input & (CROUCH | BLOCK | LEFT | RIGHT));
                f.buffer_time = 12;
            } else if self.freeze == 0 {
                f.buffer_time = f.buffer_time.saturating_sub(1);
            }
        }
        if self.freeze > 0 {
            self.freeze -= 1;
            return;
        }
        self.remaining = self.remaining.saturating_sub(1);
        self.physics();
        let mut breaker = None;
        for i in 0..2 {
            let f = &mut self.fighters[i];
            let input = inputs[i] & INPUT_MASK;
            f.invulnerable = f.invulnerable.saturating_sub(1);
            f.combo_time = f.combo_time.saturating_sub(1);
            if f.combo_time == 0 {
                f.combo = 0;
                f.combo_damage = 0;
            }
            // A full bar buys one defensive escape; not available once grounded.
            if f.stun > 0
                && f.down == 0
                && f.meter >= 1000
                && f.buffer_time > 0
                && f.buffered & (BLOCK | DASH) == (BLOCK | DASH)
            {
                f.meter = 0;
                f.stun = 0;
                f.y = 0;
                f.vy = 0;
                f.vx = -f.facing * 90;
                f.action = 0;
                f.invulnerable = 24;
                f.buffer_time = 0;
                f.juggle = 0;
                breaker = Some(i);
                continue;
            }
            if f.held > 0 {
                // In the thrower's grip: no control; slammed down at the end.
                f.held -= 1;
                f.frame += 1;
                f.action = 5;
                f.guard = false;
                f.crouch = false;
                if f.held == 0 {
                    f.down = KNOCKDOWN;
                    f.action = 15;
                    f.frame = 0;
                    f.vx = -f.facing * 30;
                }
                continue;
            }
            if f.down > 0 {
                f.guard = false;
                f.crouch = false;
                f.action = 15;
                f.down -= 1;
                f.frame = KNOCKDOWN - f.down.min(KNOCKDOWN);
                if f.down == 0 {
                    f.action = 0;
                    f.stun = 0;
                    f.invulnerable = 12;
                    f.juggle = 0;
                }
                continue;
            }
            if f.stun > 0 {
                f.stun -= 1;
                f.frame += 1;
                f.action = 5;
                f.guard = false;
                if f.stun == 0 && f.y == 0 {
                    f.action = 0;
                    f.frame = 0;
                    f.juggle = 0;
                }
                continue;
            }
            if f.blockstun > 0 {
                f.blockstun -= 1;
                continue;
            }
            if f.action == 5 {
                f.action = 0;
                f.frame = 0;
            }
            let buffered = if f.buffer_time > 0 { f.buffered } else { 0 };
            let candidate = select_action(f, buffered);
            if f.action != 0 {
                f.frame += 1;
                if f.action == 3 && f.frame < 11 {
                    f.x += f.dash_dir * 72;
                }
                let cancel = f.confirmed
                    && attack(f.action).is_some_and(|m| f.frame >= m.startup + 2)
                    && moves::cancel(f.action, candidate);
                if cancel || f.frame >= duration(f.action) {
                    f.action = 0;
                    f.frame = 0;
                }
            }
            let was_guard = f.guard;
            f.crouch = f.y == 0 && f.action == 0 && input & CROUCH != 0;
            f.guard = f.action == 0 && f.y == 0 && input & BLOCK != 0;
            f.guard_age = if f.guard {
                if was_guard {
                    f.guard_age + 1
                } else {
                    0
                }
            } else {
                0
            };
            if f.action == 0 {
                let movement = i32::from(input & RIGHT != 0) - i32::from(input & LEFT != 0);
                if f.y == 0 {
                    f.x += movement * if f.guard || f.crouch { 12 } else { 38 };
                }
                if buffered & JUMP != 0 && f.y == 0 {
                    f.vy = 112;
                    f.y = 1;
                    f.vx = movement * 42;
                    f.guard = false;
                    f.crouch = false;
                    f.air_attack = false;
                    f.buffer_time = 0;
                } else if candidate != 0 {
                    let cost = attack(candidate).map_or(160, |m| m.cost);
                    if f.stamina >= cost
                        && (candidate != 14 || f.meter >= 500)
                        && (f.y == 0 || (candidate == 13 && !f.air_attack))
                    {
                        f.stamina -= cost;
                        if candidate == 14 {
                            f.meter -= 500;
                        }
                        f.action = candidate;
                        f.frame = 0;
                        f.connected = false;
                        f.confirmed = false;
                        f.prop_hit = false;
                        f.guard = false;
                        f.crouch = false;
                        f.buffer_time = 0;
                        if f.y > 0 {
                            f.air_attack = true;
                        }
                        f.dash_dir = if movement == 0 { -f.facing } else { movement };
                    }
                }
                f.stamina = (f.stamina + if f.guard { 3 } else { 6 }).min(1000);
            }
            f.x = f.x.clamp(-ARENA_LIMIT, ARENA_LIMIT);
        }
        if let Some(side) = breaker {
            let enemy = &mut self.fighters[1 - side];
            enemy.vx = -enemy.facing * 100;
            enemy.stun = 22;
            enemy.action = 5;
            enemy.combo = 0;
            enemy.combo_damage = 0;
            self.event += 1;
            self.event_kind = 8;
            self.event_target = side;
            self.freeze = 6;
        }
        // Fighters cannot cross; facing and control direction stay predictable on phones.
        if self.fighters[1].x - self.fighters[0].x < 600 {
            let mid = ((self.fighters[0].x + self.fighters[1].x) / 2).clamp(
                -ARENA_LIMIT + 300,
                ARENA_LIMIT - 300,
            );
            self.fighters[0].x = mid - 300;
            self.fighters[1].x = mid + 300;
        }
        // Resolve both intents from the same pre-hit state: simultaneous hits trade.
        let before = self.fighters.clone();
        for side in 0..2 {
            let a = &before[side];
            let d = &before[1 - side];
            let Some(m) = attack(a.action) else { continue };
            if a.frame < m.startup || a.frame >= m.startup + m.active {
                continue;
            }
            if !a.prop_hit {
                self.damage_room(
                    if a.action == 19 {
                        a.x
                    } else {
                        a.x + a.facing * m.reach / 2
                    },
                    if a.action == 19 {
                        950
                    } else {
                        m.reach / 3 + 220
                    },
                    if a.action == 14 || a.action == 19 {
                        if a.action == 19 {
                            48
                        } else {
                            32
                        }
                    } else {
                        m.damage
                    },
                    a.facing * 100,
                );
                self.fighters[side].prop_hit = true;
            }
            if a.connected
                || d.invulnerable > 0
                || d.down > 0
                || d.juggle >= 4
                || (a.x - d.x).abs() > m.reach
                || (d.action == 3 && (2..=10).contains(&d.frame))
            {
                continue;
            }
            if (m.height == Height::High && d.crouch && !d.guard)
                || (m.height == Height::Low && d.y > 160)
                || (m.height == Height::Grab && (d.y > 0 || d.stun > 0))
                || (a.y - d.y).abs() > if a.action == 10 { 1800 } else { 950 }
            {
                continue;
            }
            self.fighters[side].connected = true;
            self.event += 1;
            self.event_target = 1 - side;
            let blocked = d.guard
                && m.height != Height::Grab
                && !(m.height == Height::Low && !d.crouch)
                && !(m.height == Height::Overhead && d.crouch);
            if blocked {
                if d.guard_age < 6 && d.blockstun == 0 {
                    self.event_kind = 3;
                    self.fighters[side].stun = 30;
                    self.fighters[side].action = 5;
                    self.fighters[side].frame = 0;
                    self.fighters[1 - side].meter = (d.meter + 130).min(1000);
                } else {
                    let defender = &mut self.fighters[1 - side];
                    defender.stamina = (defender.stamina - m.damage * 15).max(0);
                    defender.blockstun = m.blockstun;
                    defender.vx = a.facing * 18;
                    self.event_kind = 2;
                    if defender.stamina == 0 {
                        self.event_kind = 4;
                        defender.stun = 42;
                        defender.blockstun = 0;
                        defender.guard = false;
                        defender.action = 5;
                    }
                }
                self.freeze = 5;
            } else {
                // Chains are true only while the victim is still in hitstun/airborne.
                let chain = if d.stun > 0 || (d.y > 0 && d.juggle > 0) {
                    a.combo.min(5)
                } else {
                    0
                };
                let counter = attack(d.action).is_some_and(|m| d.frame < m.startup);
                let punish = attack(d.action).is_some_and(|m| d.frame >= m.startup + m.active);
                let dealt = (m.damage + if counter { 3 } else { 0 })
                    * (100 - chain as i32 * 13).max(35)
                    / 100;
                let defender = &mut self.fighters[1 - side];
                defender.hp = (defender.hp - dealt).max(0);
                defender.stun = m.stun;
                defender.action = 5;
                defender.frame = 0;
                defender.guard = false;
                defender.crouch = false;
                defender.blockstun = 0;
                defender.vx = a.facing * m.push;
                defender.recoil_v = if m.damage >= 15 { 150 } else { 80 };
                defender.meter = (defender.meter + dealt * 8).min(1000);
                if m.launch > 0 || d.y > 0 {
                    defender.juggle += 1;
                    defender.vy = if defender.juggle >= 4 {
                        -35
                    } else {
                        (m.launch.max(55) - chain as i32 * 12).max(20)
                    };
                    defender.y = defender.y.max(1);
                } else if m.knockdown {
                    defender.down = KNOCKDOWN;
                    defender.stun = 0;
                }
                if a.action == 4 {
                    // Grabbed: held in front of the thrower, then slammed.
                    defender.held = HOLD;
                    defender.stun = 0;
                    defender.down = 0;
                    defender.juggle = 0;
                    defender.vx = 0;
                    defender.vy = 0;
                    defender.y = 0;
                }
                let attacker = &mut self.fighters[side];
                attacker.combo = chain + 1;
                attacker.combo_time = 90;
                attacker.combo_damage = if chain == 0 {
                    dealt
                } else {
                    attacker.combo_damage + dealt
                };
                attacker.confirmed = true;
                attacker.meter = (attacker.meter + dealt * 6).min(1000);
                self.event_kind = if a.action == 4 {
                    5
                } else if punish {
                    7
                } else if counter {
                    6
                } else {
                    1
                };
                self.freeze = if m.damage >= 15 { 11 } else { 7 };
            }
        }
        // A held victim hangs in the thrower's grip (from the tick it is grabbed).
        for side in 0..2 {
            if self.fighters[side].held > 0 {
                let bounds = [-ARENA_LIMIT, ARENA_LIMIT];
                let (x, facing) = (self.fighters[1 - side].x, self.fighters[1 - side].facing);
                let v = &mut self.fighters[side];
                v.x = (x + facing * HOLD_DISTANCE).clamp(bounds[0], bounds[1]);
                v.y = 0;
                v.vx = 0;
                v.vy = 0;
            }
        }
        if self.remaining == 0 || self.fighters.iter().any(|f| f.hp == 0) {
            self.winner = match self.fighters[0].hp.cmp(&self.fighters[1].hp) {
                std::cmp::Ordering::Greater => 0,
                std::cmp::Ordering::Less => 1,
                _ => -1,
            };
            if self.winner >= 0 {
                self.score[self.winner as usize] += 1;
            }
            self.phase = if self.score.iter().any(|s| *s >= 2) {
                3
            } else {
                2
            };
            self.phase_ticks = 150;
        }
    }
    fn damage_room(&mut self, x: i32, radius: i32, damage: i32, impulse: i32) {
        for (o, def) in self.objects.iter_mut().zip(room::LAYOUT) {
            if o.hp > 0 && (def.x - x).abs() <= radius {
                let material_damage = match def.kind {
                    // The exterior shell remains intact throughout the fight.
                    0 => 0,
                    8 => damage / 4,
                    7 => {
                        if damage >= 45 {
                            damage / 2
                        } else {
                            0
                        }
                    }
                    5 => {
                        if damage >= 30 {
                            damage / 3
                        } else {
                            0
                        }
                    }
                    _ => damage,
                };
                o.hp = (o.hp - material_damage).max(0);
                if o.hp == 0 {
                    o.broken_tick = self.tick;
                    o.impulse = impulse;
                }
            }
        }
    }
    fn physics(&mut self) {
        for i in 0..2 {
            let f = &self.fighters[i];
            if f.stun > 0 && f.vx.abs() > 55 && f.room_cooldown == 0 {
                self.damage_room(f.x, 380, 8, f.vx);
                self.fighters[i].room_cooldown = 30;
            }
        }
        for side in 0..2 {
            let f = &mut self.fighters[side];
            f.wall_cooldown = f.wall_cooldown.saturating_sub(1);
            f.room_cooldown = f.room_cooldown.saturating_sub(1);
            f.recoil_v = (f.recoil_v - f.recoil * 7 / 100) * 82 / 100;
            f.recoil = (f.recoil + f.recoil_v).clamp(-120, 550);
            if f.recoil.abs() < 20 && f.recoil_v.abs() < 3 {
                f.recoil = 0;
                f.recoil_v = 0;
            }
            f.x += f.vx;
            f.vx = f.vx * if f.y > 0 { 97 } else { 87 } / 100;
            if f.vx.abs() < 2 {
                f.vx = 0;
            }
            let airborne = f.y > 0;
            f.y = (f.y + f.vy).max(0);
            if f.y > 0 {
                f.vy -= 5;
            } else {
                f.vy = 0;
                if airborne {
                    f.air_attack = false;
                    if f.juggle > 0 {
                        f.down = KNOCKDOWN;
                        f.stun = 0;
                        f.action = 15;
                        f.juggle = 0;
                    } else if f.action == 13 {
                        f.action = 0;
                        f.frame = 0;
                    }
                }
            }
            for wall_side in 0..2 {
                let sign = if wall_side == 0 { -1 } else { 1 };
                let wall = &mut self.walls[wall_side];
                let outward = f.vx * sign;
                if wall.hp > 0 && f.x * sign >= ARENA_LIMIT {
                    f.x = sign * ARENA_LIMIT;
                    if outward >= 35 && f.wall_cooldown == 0 {
                        wall.impacts += 1;
                        wall.impulse = outward;
                        f.wall_cooldown = 30;
                        f.stun = f.stun.max(24);
                        f.recoil_v = -80;
                        f.vx = -sign * outward / 3;
                    } else {
                        f.vx = 0;
                    }
                }
            }
            f.x = f.x.clamp(-ARENA_LIMIT, ARENA_LIMIT);
        }
    }
}
fn select_action(f: &Fighter, bits: u32) -> u32 {
    if bits == 0 {
        return 0;
    }
    if bits & DASH != 0 {
        return 3;
    }
    if bits & SPECIAL != 0 {
        return 14;
    }
    if bits & THROW != 0 {
        return 4;
    }
    if bits & SMASH != 0 {
        return 19;
    }
    if bits & KICK != 0 {
        return if f.y > 0 {
            13
        } else if bits & CROUCH != 0 {
            9
        } else if f.action == 11 {
            12
        } else {
            8
        };
    }
    if bits & HEAVY != 0 {
        return if bits & CROUCH != 0 || f.action == 8 || f.action == 11 {
            10
        } else {
            2
        };
    }
    if bits & LIGHT != 0 {
        return if f.y > 0 {
            13
        } else if f.action == 1 {
            11
        } else {
            1
        };
    }
    0
}
pub fn duration(action: u32) -> u32 {
    attack(action).map_or(if action == 3 { 24 } else { 0 }, |m| m.total)
}

// Tiny standalone ABI: one isolated WASM instance per match, in Node and browser.
#[cfg(target_arch = "wasm32")]
mod abi {
    use super::*;
    use std::cell::RefCell;
    thread_local! {
        static GAME: RefCell<Match> = RefCell::new(Match::default());
        static JSON: RefCell<String> = const { RefCell::new(String::new()) };
    }
    #[no_mangle]
    pub extern "C" fn arena_reset(seed: u32) {
        GAME.with(|g| *g.borrow_mut() = Match::new(seed));
    }
    #[no_mangle]
    pub extern "C" fn arena_step(a: u32, b: u32) {
        GAME.with(|g| g.borrow_mut().step([a, b]));
    }
    #[no_mangle]
    pub extern "C" fn arena_bot(side: u32) -> u32 {
        GAME.with(|g| g.borrow_mut().bot_input((side as usize).min(1)))
    }
    #[no_mangle]
    pub extern "C" fn arena_forfeit(side: u32) {
        GAME.with(|g| g.borrow_mut().forfeit(side as usize));
    }
    #[no_mangle]
    pub extern "C" fn arena_state() -> *const u8 {
        GAME.with(|g| {
            JSON.with(|j| {
                *j.borrow_mut() = g.borrow().serialize_json();
                j.borrow().as_ptr()
            })
        })
    }
    #[no_mangle]
    pub extern "C" fn arena_state_len() -> usize {
        JSON.with(|j| j.borrow().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn duel() -> Match {
        let mut m = Match::new(42);
        m.phase = 1;
        m.fighters[0].x = -400;
        m.fighters[1].x = 400;
        m
    }
    fn run(m: &mut Match, inputs: [u32; 2], n: usize) {
        for _ in 0..n {
            m.step(inputs);
        }
    }
    #[test]
    fn hit_only_once_and_held_button_does_not_repeat() {
        let mut m = duel();
        run(&mut m, [LIGHT, 0], 100);
        assert_eq!(m.fighters[1].hp, 92);
    }
    #[test]
    fn range_matters() {
        let mut m = Match::default();
        m.phase = 1;
        run(&mut m, [HEAVY, 0], 60);
        assert_eq!(m.fighters[1].hp, 100);
    }
    #[test]
    fn heavy_hit_has_momentum_gravity_and_wall_collision() {
        let mut m = duel();
        m.fighters[0].x = ARENA_LIMIT - 1100;
        m.fighters[1].x = ARENA_LIMIT - 150;
        run(&mut m, [HEAVY, 0], 45);
        assert_eq!(m.walls[1].hp, 110);
        assert_eq!(m.walls[1].impacts, 1);
        assert!(m.fighters[1].hp < 100);
        run(&mut m, [0, 0], 100);
        assert_eq!(m.fighters[1].y, 0);
        assert_eq!(m.fighters[1].vy, 0);
        assert!(m.fighters[1].recoil.abs() < 10);
    }
    #[test]
    fn every_room_is_reachable_without_breaking_anything() {
        let mut m = duel();
        run(&mut m, [LEFT, RIGHT], 500);
        assert_eq!(m.fighters[0].x, -ARENA_LIMIT);
        assert_eq!(m.fighters[1].x, ARENA_LIMIT);
        assert!(m.objects.iter().zip(room::LAYOUT).all(|(o, d)| o.hp == d.hp));
        assert!(m.walls.iter().all(|w| w.hp == 110 && w.broken_tick == 0));
        // Draws exercise all five starting rooms and the wrap back to living.
        for center in room::ROUND_CENTERS.iter().cycle().skip(1).take(5) {
            m.remaining = 1;
            m.step([0, 0]);
            run(&mut m, [0, 0], 270);
            assert_eq!(m.fighters[0].x, center - 1150);
            assert_eq!(m.fighters[1].x, center + 1150);
        }
    }
    #[test]
    fn running_into_wall_cannot_break_it_without_hit_impulse() {
        let mut m = duel();
        run(&mut m, [LEFT, RIGHT], 600);
        assert_eq!(m.walls[0].hp, 110);
        assert_eq!(m.walls[1].hp, 110);
    }
    #[test]
    fn exhausted_guard_breaks_and_recovers() {
        let mut m = duel();
        run(&mut m, [0, BLOCK], 10);
        m.fighters[1].stamina = 10;
        run(&mut m, [HEAVY, BLOCK], 24);
        assert_eq!(m.event_kind, 4);
        assert!(m.fighters[1].stun > 0);
        assert!(!m.fighters[1].guard);
        run(&mut m, [0, 0], 90);
        assert_eq!(m.fighters[1].stun, 0);
        assert!(m.fighters[1].stamina > 0);
    }
    #[test]
    fn confirmed_jab_can_cancel_but_whiff_cannot() {
        let mut m = duel();
        run(&mut m, [LIGHT, 0], 13);
        run(&mut m, [HEAVY, 0], 7);
        assert_eq!(m.fighters[0].action, 2);
        let mut far = Match::default();
        far.phase = 1;
        run(&mut far, [LIGHT, 0], 13);
        run(&mut far, [HEAVY, 0], 7);
        assert_eq!(far.fighters[0].action, 1);
    }
    #[test]
    fn draw_does_not_award_points_and_forfeit_is_final() {
        let mut m = duel();
        m.remaining = 1;
        m.step([0, 0]);
        assert_eq!(m.winner, -1);
        assert_eq!(m.score, [0, 0]);
        m.forfeit(0);
        let final_state = m.serialize_json();
        run(&mut m, [HEAVY, HEAVY], 100);
        assert_eq!(m.serialize_json(), final_state);
        assert_eq!(m.winner, 1);
    }
    #[test]
    fn block_parry_and_throw_are_distinct() {
        let mut m = duel();
        run(&mut m, [0, BLOCK], 10);
        run(&mut m, [LIGHT, BLOCK], 20);
        assert_eq!(m.fighters[1].hp, 100);
        assert_eq!(m.event_kind, 2);
        let mut m = duel();
        run(&mut m, [LIGHT, 0], 5);
        run(&mut m, [0, BLOCK], 4);
        assert_eq!(m.event_kind, 3);
        assert!(m.fighters[0].stun > 0);
        let mut m = duel();
        run(&mut m, [THROW, BLOCK], 20);
        assert_eq!(m.fighters[1].hp, 83);
    }
    #[test]
    fn thrown_body_lands_before_the_thrower_recovers() {
        let mut m = duel();
        m.step([THROW, 0]);
        let (mut landed, mut recovered) = (None, None);
        let mut held = 0;
        for tick in 1..120 {
            m.step([0, 0]);
            let (a, d) = (&m.fighters[0], &m.fighters[1]);
            if d.held > 0 {
                // Pinned in the thrower's grip, feet on the floor.
                held += 1;
                assert_eq!((d.x - a.x).abs(), HOLD_DISTANCE);
                assert_eq!(d.y, 0);
            }
            if landed.is_none() && d.down > 0 && d.y == 0 {
                landed = Some(tick);
            }
            if recovered.is_none() && a.action == 0 {
                recovered = Some(tick);
            }
        }
        assert!(held + 1 >= HOLD as usize, "held for {held} ticks");
        let (landed, recovered) = (landed.expect("thrown"), recovered.expect("recovered"));
        assert!(landed < recovered, "lands on {landed}, thrower free on {recovered}");
        assert_eq!(m.fighters[1].hp, 83);
    }
    #[test]
    fn airborne_fighters_land_after_the_round_and_the_match() {
        for last_round in [false, true] {
            let mut m = duel();
            if last_round {
                m.score = [1, 0];
            }
            m.fighters[0].y = 900;
            m.fighters[0].vy = 40;
            m.fighters[0].vx = 30;
            m.fighters[1].hp = 1;
            m.remaining = 1;
            m.step([0, 0]);
            assert!(m.phase >= 2);
            for _ in 0..60 {
                m.step([0, 0]);
            }
            assert!(m.phase >= 2);
            assert_eq!(m.fighters[0].y, 0, "phase {}", m.phase);
        }
    }
    #[test]
    fn dash_evades_and_costs_stamina() {
        let mut m = duel();
        run(&mut m, [LIGHT, DASH], 14);
        assert_eq!(m.fighters[1].hp, 100);
        assert!(m.fighters[1].stamina < 1000);
    }
    #[test]
    fn simultaneous_hits_trade() {
        let mut m = duel();
        run(&mut m, [LIGHT, LIGHT], 12);
        assert_eq!(m.fighters[0].hp, 92);
        assert_eq!(m.fighters[1].hp, 92);
    }
    #[test]
    fn timeout_and_first_to_two() {
        let mut m = duel();
        m.remaining = 1;
        m.fighters[1].hp = 20;
        m.step([0, 0]);
        assert_eq!(m.score, [1, 0]);
        run(&mut m, [0, 0], 270);
        assert_eq!(m.phase, 1);
        assert_eq!(m.fighters[1].hp, 100);
        m.remaining = 1;
        m.fighters[1].hp = 20;
        m.step([0, 0]);
        assert_eq!(m.phase, 3);
        assert_eq!(m.winner, 0);
    }
    #[test]
    fn walls_and_no_crossing() {
        let mut m = duel();
        run(&mut m, [RIGHT, LEFT], 500);
        assert!(m.fighters[1].x - m.fighters[0].x >= 600);
        run(&mut m, [LEFT, RIGHT], 500);
        assert_eq!(m.fighters[0].x, -ARENA_LIMIT);
        assert_eq!(m.fighters[1].x, ARENA_LIMIT);
    }
    #[test]
    fn high_low_overhead_defences_have_real_tradeoffs() {
        let mut standing = duel();
        run(&mut standing, [0, BLOCK], 10);
        run(&mut standing, [CROUCH | KICK, BLOCK], 25);
        assert_eq!(standing.fighters[1].hp, 87);
        let mut low = duel();
        run(&mut low, [0, CROUCH | BLOCK], 10);
        run(&mut low, [CROUCH | KICK, CROUCH | BLOCK], 25);
        assert_eq!(low.fighters[1].hp, 100);
        let mut overhead = duel();
        run(&mut overhead, [0, CROUCH | BLOCK], 10);
        run(&mut overhead, [HEAVY, CROUCH | BLOCK], 30);
        assert!(overhead.fighters[1].hp < 100);
        let mut duck = duel();
        run(&mut duck, [LIGHT, CROUCH], 20);
        assert_eq!(duck.fighters[1].hp, 100);
    }
    #[test]
    fn three_hit_string_is_confirmed_scaled_and_buffered_through_hitstop() {
        let mut m = duel();
        run(&mut m, [LIGHT, 0], 8);
        assert_eq!(m.fighters[1].hp, 92);
        m.step([0, 0]);
        m.step([LIGHT, 0]); // second press during hitstop
        for _ in 0..30 {
            m.step([0, 0]);
            if m.fighters[0].action == 11 && m.fighters[0].confirmed {
                break;
            }
        }
        assert_eq!(m.fighters[0].combo, 2);
        m.step([KICK, 0]);
        for _ in 0..35 {
            m.step([0, 0]);
            if m.fighters[0].combo == 3 {
                break;
            }
        }
        assert_eq!(m.fighters[0].combo, 3);
        assert!(m.fighters[0].combo_damage < 8 + 9 + 17);
        assert_eq!(m.fighters[1].hp, 100 - m.fighters[0].combo_damage);
    }
    #[test]
    fn blocked_heavy_has_a_punishable_recovery() {
        let mut m = duel();
        run(&mut m, [0, BLOCK], 10);
        run(&mut m, [HEAVY, BLOCK], 27);
        run(&mut m, [0, BLOCK], 14);
        run(&mut m, [0, LIGHT], 10);
        assert!(m.fighters[0].hp < 100);
        assert_eq!(m.event_kind, 7);
    }
    #[test]
    fn jump_evades_sweep_and_lands_without_repeating_air_attack() {
        let mut m = duel();
        m.step([JUMP, CROUCH | KICK]);
        run(&mut m, [KICK, 0], 24);
        assert!(m.fighters[0].y > 0);
        assert_eq!(m.fighters[0].hp, 100);
        run(&mut m, [0, 0], 80);
        assert_eq!(m.fighters[0].y, 0);
        assert!(!m.fighters[0].air_attack);
    }
    #[test]
    fn breaker_requires_full_meter_and_interrupts_enemy_combo() {
        let mut m = duel();
        m.fighters[1].stun = 20;
        m.fighters[1].meter = 999;
        m.step([0, BLOCK | DASH]);
        assert!(m.fighters[1].stun > 0);
        m.step([0, 0]);
        m.fighters[1].meter = 1000;
        m.step([0, BLOCK | DASH]);
        assert_eq!(m.event_kind, 8);
        assert_eq!(m.fighters[1].stun, 0);
        assert_eq!(m.fighters[1].meter, 0);
        assert!(m.fighters[0].stun > 0);
    }
    #[test]
    fn special_costs_meter_once_and_cannot_be_spammed() {
        let mut m = duel();
        m.fighters[0].meter = 0;
        run(&mut m, [SPECIAL, 0], 60);
        assert_eq!(m.fighters[1].hp, 100);
        m.step([0, 0]);
        m.fighters[0].meter = 500;
        run(&mut m, [SPECIAL, 0], 70);
        assert!(m.fighters[1].hp < 100);
        assert!(m.fighters[0].meter < 500);
    }
    #[test]
    fn furniture_breaks_but_exterior_stays_intact() {
        let mut m = duel();
        m.remaining = 100_000;
        for id in 5..room::OBJECTS {
            let x = room::LAYOUT[id].x;
            m.fighters[0].x = (x - 400).clamp(-ARENA_LIMIT + 300, ARENA_LIMIT - 300);
            m.fighters[1].x = ARENA_LIMIT;
            m.fighters[1].invulnerable = 1000;
            for _ in 0..7 {
                m.fighters[0].stamina = 1000;
                run(&mut m, [SMASH, 0], 55);
                m.step([0, 0]);
            }
            assert_eq!(m.objects[id].hp, 0, "object {id}");
            assert!(m.objects[id].broken_tick > 0);
        }
        assert!(m.objects[..5]
            .iter()
            .zip(room::LAYOUT)
            .all(|(o, d)| o.hp == d.hp));
        m.remaining = 1;
        m.step([0, 0]);
        run(&mut m, [0, 0], 150);
        assert!(m
            .objects
            .iter()
            .zip(room::LAYOUT)
            .all(|(o, d)| o.hp == d.hp && o.broken_tick == 0));
    }
    #[test]
    fn a_jab_does_not_destroy_the_room_and_body_contact_is_cooldown_limited() {
        let mut m = duel();
        run(&mut m, [LIGHT, 0], 16);
        assert!(m.objects.iter().all(|o| o.broken_tick == 0));
        let before = m.objects[7].hp;
        m.fighters[0].x = room::LAYOUT[7].x;
        m.fighters[0].vx = 80;
        m.fighters[0].stun = 60;
        m.fighters[0].room_cooldown = 0;
        m.physics();
        let once = m.objects[7].hp;
        for _ in 0..10 {
            m.physics();
        }
        assert!(once < before);
        assert_eq!(m.objects[7].hp, once);
    }
    #[test]
    fn air_combo_limit_forces_landing_and_safe_wakeup() {
        let mut m = duel();
        m.fighters[1].y = 600;
        m.fighters[1].vy = -10;
        m.fighters[1].juggle = 4;
        m.fighters[1].stun = 30;
        run(&mut m, [LIGHT, 0], 12);
        assert_eq!(m.fighters[1].hp, 100);
        for _ in 0..120 {
            m.step([0, 0]);
            if m.fighters[1].invulnerable > 0 {
                break;
            }
        }
        assert_eq!(m.fighters[1].y, 0);
        assert_eq!(m.fighters[1].down, 0);
        assert!(m.fighters[1].invulnerable > 0);
    }
    #[test]
    fn deterministic_long_match_and_serialization() {
        let mut a = Match::new(77);
        let mut b = a.clone();
        for _ in 0..15000 {
            let x = a.bot_input(0);
            let y = a.bot_input(1);
            assert_eq!(x, b.bot_input(0));
            assert_eq!(y, b.bot_input(1));
            a.step([x, y]);
            b.step([x, y]);
        }
        assert_eq!(a.serialize_json(), b.serialize_json());
        assert!(Match::deserialize_json(&a.serialize_json()).is_ok());
    }
}
