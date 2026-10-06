//! Authoritative, fixed 60 Hz combat. No clock, graphics, network or random IO.
use nanoserde::{DeJson, SerJson};
pub mod boxes;
pub mod moves;
pub mod room;
use moves::{attack, Height, Move};

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
/// Held with forward: run (`moves::run`); with back: back off quicker.
pub const RUN: u32 = 4096;
pub const INPUT_MASK: u32 = 8191;
/// Stamina a tick of running costs (it does not recover while running).
pub const RUN_COST: i32 = 2;
/// Forward speed a running fighter carries into an attack (mm/tick).
pub const RUN_CARRY: i32 = 28;
pub const EDGE_MASK: u32 = LIGHT | HEAVY | DASH | THROW | KICK | JUMP | SPECIAL | SMASH;
/// Ticks on the floor after a knockdown, including the visible wake-up.
pub const KNOCKDOWN: u32 = 56;
/// A guard raised this many ticks before the blow parries it.
pub const PARRY: u32 = 6;
/// Walking speed of the all-round style (mm/tick): 1.44 m/s forward, 1.2 m/s
/// back (other styles: `moves::walk`); 0.72 m/s guarded or crouched for all.
pub const WALK_FORWARD: i32 = 24;
pub const WALK_BACK: i32 = 20;
pub const WALK_SLOW: i32 = 12;
/// After lowering the guard, raising it again within this many ticks blocks
/// without the parry window: tapping the button is not a free parry.
pub const PARRY_COOLDOWN: u32 = 18;
/// Hit-stop (both fighters frozen, ticks): a light and a heavy hit, a
/// counter hit adds a tick; a blocked blow, a parry, a grab or a broken
/// throw. Short punctuation: on phones a longer stop read as the game hanging.
pub const STOP_LIGHT: u32 = 5;
pub const STOP_HEAVY: u32 = 8;
pub const STOP_COUNTER: u32 = 1;
pub const STOP_BLOCK: u32 = 3;
pub const STOP_PARRY: u32 = 6;
pub const STOP_GRAB: u32 = 5;
/// A counter hit (a blow into the startup of the opponent's attack) stuns
/// this much longer: room for a follow-up the plain hit does not give.
pub const COUNTER_STUN: u32 = 12;
/// A counter-hit kick or hook leaves the victim reeling in place this long:
/// a step in and a free combo for whoever read the attack.
pub const CRUMPLE: u32 = 56;
/// From this frame of a knockdown (on the floor), pressing up stands the
/// fighter up twice as fast, without the wake-up protection.
pub const QUICK_RISE_FROM: u32 = 20;
/// Wake-up protection after a normal and after a quick rise (ticks).
pub const WAKE_GUARD: u32 = 12;
pub const QUICK_WAKE_GUARD: u32 = 3;
use room::ARENA_LIMIT;
use moves::TECH;
// action: 0 idle, 1 jab, 2 heavy, 3 dash, 4 throw, 5 stun (see PROTOCOL.md)
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
    /// Ticks until a raised guard gets its parry window again.
    #[nserde(default)]
    pub parry_cooldown: u32,
    /// Fighting style (`moves::ALLROUND`, `PRESSURE`, `RANGE`).
    #[nserde(default)]
    pub style: u32,
    /// Getting up quickly (pressed up while on the floor).
    #[nserde(default)]
    pub quick_rise: bool,
    /// Running forward this tick.
    #[nserde(default)]
    pub running: bool,
}
/// A throw holds its victim this long, from the grab to the slam.
pub const HOLD: u32 = 32;
/// The slammed victim starts its knockdown this far in (it lands on the
/// floor out of the grip, not from standing).
pub const THROWN_EARLY: u32 = 14;
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
            parry_cooldown: 0,
            style: 0,
            quick_rise: false,
            running: false,
        }
    }
    /// The attack this fighter is performing, with its style's frame data.
    pub fn attack(&self) -> Option<Move> {
        moves::attack_for(self.style, self.action)
    }
    /// Ticks `action` lasts for this fighter.
    pub fn duration(&self, action: u32) -> u32 {
        moves::attack_for(self.style, action).map_or(if action == 3 { 24 } else { 0 }, |m| m.total)
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
    // 1 hit, 2 block, 3 parry, 4 guard break, 5 grab, 6 counter, 7 punish,
    // 8 combo breaker, 9 throw broken (tech), 10 throw slam, 11 counter-hit
    // crumple (the victim reels for CRUMPLE ticks)
    pub event_kind: u32,
    pub event_target: usize,
    pub seed: u32,
    /// Each side's fighting style, kept across rounds.
    #[nserde(default)]
    pub styles: [u32; 2],
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
            styles: [0, 0],
        }
    }
    /// Sets a side's fighting style (from its fighter, roster.json).
    pub fn set_style(&mut self, side: usize, style: u32) {
        if side < 2 {
            self.styles[side] = style.min(moves::RANGE);
            self.fighters[side].style = self.styles[side];
        }
    }
    pub fn forfeit(&mut self, loser: usize) {
        if loser < 2 && self.phase != 3 {
            self.winner = (1 - loser) as i32;
            self.phase = 3;
        }
    }
    /// The sparring bot's habit this round (`Habit`): a tendency a player
    /// can notice and punish, different from round to round.
    pub fn bot_habit(&self, side: usize) -> u32 {
        mix(self.round.wrapping_mul(0x9E37) ^ self.seed ^ (side as u32).wrapping_mul(0x51ED)) % 5
    }
    /// Sparring bot with human limits: it sees an attack only after ~200 ms,
    /// guards about half the time it could (sometimes at the wrong height),
    /// never parries on reaction, moves in and out and attacks often. Its
    /// plans come from a hash of short time windows, so they hold for a
    /// while without extra state. Each round it leans on one habit
    /// (`bot_habit`): jumping in, turtling, grabbing or swinging heavies.
    pub fn bot_input(&self, side: usize) -> u32 {
        let distance = (self.fighters[0].x - self.fighters[1].x).abs();
        let toward = if side == 0 { RIGHT } else { LEFT };
        let away = if side == 0 { LEFT } else { RIGHT };
        let enemy = self.fighters[1 - side].clone();
        let me = self.fighters[side].clone();
        // Range pokes from further out; pressure pokes like the all-round
        // style but dashes in instead some of the time.
        let (poke, far) = match me.style {
            moves::RANGE => (1450, 1800),
            _ => (1350, 1700),
        };
        let salt = self.round.wrapping_mul(7919) ^ (side as u32).wrapping_mul(104_729) ^ self.seed;
        let roll = |key: u32| mix(key ^ salt) % 100;
        let habit = self.bot_habit(side);
        // Grabbed: breaks the throw about one time in four.
        if me.held > 0 {
            return if me.held == HOLD - 4 && roll(self.tick) < 25 { THROW } else { 0 };
        }
        // On the floor: gets up quickly about four times in ten.
        if me.down > 0 {
            let quick = roll(self.tick.wrapping_sub(me.frame) ^ 0x0B) < 40;
            return if quick && me.frame == QUICK_RISE_FROM + 1 { JUMP } else { 0 };
        }
        // Its uppercut landed: half the time it jumps after the body and kicks.
        if me.action == 10 && me.confirmed && me.attack().is_some_and(|m| me.frame == m.startup + m.active + 1) {
            if roll(self.tick ^ 0xC4A5E) < 50 {
                return JUMP | toward;
            }
        }
        if me.y > 0 && !me.air_attack && enemy.juggle > 0 && enemy.y > 0 && distance < 1500 {
            return KICK;
        }
        // In the air: any of the three jump attacks.
        if me.y > 200 && !me.air_attack && distance < 1500 {
            return match roll(self.tick.wrapping_sub(me.frame) ^ 0xA1) {
                0..=49 => KICK,
                50..=74 => HEAVY,
                _ => LIGHT,
            };
        }
        // A full bar breaks a real combo now and then, a moment after a hit.
        if (me.stun > 0 || (me.juggle > 0 && me.y > 0))
            && me.meter >= 1000
            && enemy.combo >= 2
            && me.frame == 6
            && roll(self.tick.wrapping_sub(me.frame) ^ 0xB4) < 25
        {
            return BLOCK | DASH;
        }
        // A blocked blow: keep the guard until it can act again.
        if me.blockstun > 0 {
            return BLOCK | if me.crouch { CROUCH } else { 0 };
        }
        // Strings: always on a hit, sometimes on a block or a whiff; the
        // ender varies (hook, roundhouse, uppercut; side kick).
        let string = me.confirmed || roll(self.tick / 8 ^ 0x51) < if me.connected { 45 } else { 25 };
        let tap = |bits: u32| if me.previous & bits == 0 { bits } else { 0 };
        if string && me.attack().is_some_and(|m| me.frame >= m.startup + 2) {
            let ender = roll(self.tick.wrapping_sub(me.frame) ^ 0xE7);
            match me.action {
                1 => return tap(LIGHT),
                11 if me.confirmed && ender < 35 => return tap(KICK),
                11 if me.confirmed && ender < 55 && distance < 1200 => return tap(HEAVY),
                11 if ender < 75 => return tap(LIGHT),
                8 if ender < 60 => return tap(KICK),
                18 if me.confirmed && ender < 40 => return tap(KICK),
                16 if me.confirmed && distance < 1150 => return CROUCH | HEAVY,
                _ => {}
            }
        }
        // Pressure: a dash in ends in a jab, a low kick or a grab.
        if me.action == 3 && me.dash_dir == me.facing && me.frame >= moves::DASH_CANCEL && distance < 1150 {
            return match roll(self.tick.wrapping_sub(me.frame) ^ 0xDA) {
                0..=44 => LIGHT,
                45..=69 => CROUCH | LIGHT,
                _ if distance < 1000 => THROW,
                _ => LIGHT,
            };
        }
        let window = (self.tick + side as u32 * 7) / 15;
        // A jump coming in: an uppercut now and then, otherwise a guard.
        if enemy.y > 300 && me.y == 0 && distance < 1500 {
            let r = roll(window ^ 0xA11);
            if r < 25 {
                return CROUCH | HEAVY;
            } else if r < 40 {
                return away | LIGHT;
            } else if r < 70 {
                return BLOCK;
            }
        }
        if let Some(m) = enemy.attack() {
            // Human reaction: seen for 12 ticks, still 7 left (a plain block,
            // never a parry); about half of them, a quarter at the wrong height.
            let seen = enemy.frame >= 12 && enemy.frame + PARRY < m.startup;
            let key = self.tick.wrapping_sub(enemy.frame) / 16 ^ enemy.action;
            if seen && m.height != Height::Grab && roll(key) < 50 {
                let low = (m.height == Height::Low) != (roll(key ^ 0x5EED) < 25);
                return BLOCK | if low { CROUCH } else { 0 };
            }
            // A slow attack whiffed or blocked: punish it — a jab close by, a
            // kick into the stretched limb or the body further out, or a dash
            // in (pressure).
            let recovering = enemy.frame >= m.startup + m.active && m.total - enemy.frame >= 10;
            if recovering && roll(key ^ 0xBEEF) < 45 {
                let kick = moves::attack_for(me.style, 8).map_or(0, |k| k.reach);
                if distance < 1200 {
                    return if me.previous & LIGHT == 0 { LIGHT } else { 0 };
                } else if distance < kick + m.reach / 3 {
                    return if me.previous & KICK == 0 { KICK } else { 0 };
                } else if me.style == moves::PRESSURE && distance < 2200 {
                    return DASH | toward;
                }
            }
        }
        if me.stamina < 200 {
            return away;
        }
        let plan = roll(window);
        let opening = (self.tick + side as u32 * 7) % 15 == 0;
        // Habits: 1 jumps in, 2 turtles, 3 grabs, 4 swings heavies.
        let jump_in = if habit == 1 { 60 } else { 84 };
        if habit == 1 && opening && distance > 1300 && distance < far + 600 && plan >= jump_in {
            return JUMP | toward;
        }
        if distance > far {
            return match plan {
                // Far off it runs in, closer it walks.
                0..=74 if distance > far + 1200 => toward | RUN,
                0..=74 => toward,
                75..=84 if opening && distance < far + 600 => JUMP | toward,
                85..=89 if opening && me.style == moves::PRESSURE => DASH | toward,
                _ => 0,
            };
        }
        if habit == 2 && (36..=79).contains(&plan) {
            // A turtle: a long guard, rarely the first to strike.
            return BLOCK | if roll(window ^ 0xC0) < 35 { CROUCH } else { 0 };
        }
        if habit == 3 && opening && distance < 1050 && roll(window ^ 0x6A) < 45 {
            return THROW;
        }
        if habit == 4 && opening && plan < 50 && distance < 1900 {
            return if roll(window ^ 0x4E) < 60 { HEAVY } else { KICK };
        }
        match plan {
            0..=35 => {
                if !opening {
                    return 0;
                }
                let pick = roll(window ^ 0x77);
                if distance > poke && me.style == moves::PRESSURE {
                    // Pressure dashes in some of the time instead of poking.
                    match pick {
                        0..=24 => DASH | toward,
                        25..=54 => KICK,
                        55..=69 => HEAVY,
                        70..=79 if me.meter >= 500 => SPECIAL,
                        _ => toward,
                    }
                } else if distance > poke {
                    match pick {
                        0..=34 => KICK,
                        35..=49 => toward | KICK,
                        50..=62 => HEAVY,
                        63..=68 => toward | HEAVY,
                        69..=76 if me.meter >= 500 => toward | SPECIAL,
                        77..=82 if me.meter >= 500 => SPECIAL,
                        _ => toward,
                    }
                } else {
                    match pick {
                        0..=24 => LIGHT,
                        25..=32 => toward | LIGHT,
                        33..=44 => CROUCH | LIGHT,
                        45..=55 => KICK,
                        56..=61 => away | KICK,
                        62..=69 => CROUCH | KICK,
                        70..=77 => HEAVY,
                        78..=89 if distance < 1050 => THROW,
                        90..=94 => CROUCH | HEAVY,
                        _ => LIGHT,
                    }
                }
            }
            36..=55 => toward,
            56..=67 => away,
            // Holds a guard for a while: a throw or the other height opens it.
            68..=79 => BLOCK | if roll(window ^ 0xC0) < 35 { CROUCH } else { 0 },
            80..=92 => 0,
            _ => {
                if opening && roll(window ^ 0xD5) < 50 {
                    away | DASH
                } else {
                    0
                }
            }
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
    /// A broken throw (tech or two grabs at once): no damage, both fighters
    /// stagger apart and the exchange starts over from neutral.
    fn break_throw(&mut self, victim: usize) {
        for f in &mut self.fighters {
            f.held = 0;
            f.down = 0;
            f.action = 5;
            f.frame = 0;
            f.stun = 14;
            f.vx = -f.facing * 75;
            f.guard = false;
            f.crouch = false;
            f.buffer_time = 0;
        }
        self.event += 1;
        self.event_kind = 9;
        self.event_target = victim;
        self.freeze = STOP_GRAB;
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
                    for side in 0..2 {
                        self.fighters[side].style = self.styles[side];
                    }
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
        let mut tech = None;
        let mut slam = None;
        for i in 0..2 {
            let f = &mut self.fighters[i];
            let input = inputs[i] & INPUT_MASK;
            let was_running = f.running;
            f.running = false;
            f.invulnerable = f.invulnerable.saturating_sub(1);
            f.parry_cooldown = f.parry_cooldown.saturating_sub(1);
            f.combo_time = f.combo_time.saturating_sub(1);
            if f.combo_time == 0 {
                f.combo = 0;
                f.combo_damage = 0;
            }
            // A full bar buys one defensive escape; not available once grounded.
            if (f.stun > 0 || (f.juggle > 0 && f.y > 0))
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
                // In the thrower's grip: THROW right after the grab breaks
                // free; otherwise no control and slammed down at the end.
                if f.held > HOLD - TECH && f.buffer_time > 0 && f.buffered & THROW != 0 {
                    tech = Some(i);
                    continue;
                }
                f.held -= 1;
                f.frame += 1;
                f.action = 5;
                f.guard = false;
                f.crouch = false;
                if f.held == 0 {
                    // Already on the floor from the grip: a shorter lie.
                    f.down = KNOCKDOWN - THROWN_EARLY;
                    f.action = 15;
                    f.frame = 0;
                    f.vx = -f.facing * 30;
                    slam = Some(i);
                }
                continue;
            }
            if f.down > 0 {
                f.guard = false;
                f.crouch = false;
                f.action = 15;
                // Up on the floor: a quick rise (the get-up plays twice as fast).
                if !f.quick_rise && f.frame >= QUICK_RISE_FROM && f.buffer_time > 0 && f.buffered & JUMP != 0 {
                    f.quick_rise = true;
                    f.buffer_time = 0;
                }
                f.down = f.down.saturating_sub(if f.quick_rise { 2 } else { 1 });
                f.frame = KNOCKDOWN - f.down.min(KNOCKDOWN);
                if f.down == 0 {
                    f.action = 0;
                    f.stun = 0;
                    f.invulnerable = if f.quick_rise { QUICK_WAKE_GUARD } else { WAKE_GUARD };
                    f.quick_rise = false;
                    f.juggle = 0;
                }
                continue;
            }
            if f.stun > 0 || (f.juggle > 0 && f.y > 0) {
                // Hit in the air: no control until the body lands (and is
                // knocked down), so a launch is not escaped by an air attack.
                f.stun = f.stun.saturating_sub(1);
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
                    f.x += f.dash_dir * moves::dash(f.style, f.dash_dir == f.facing);
                }
                f.x += f.facing * moves::advance(f.action, f.frame);
                // Combos continue on hit, light strings also on block, and a
                // repeated button continues its string even on a whiff.
                let cancel = f.attack().is_some_and(|m| {
                    if f.confirmed {
                        f.frame >= m.startup + 2 && moves::cancel(f.action, candidate)
                    } else if f.connected {
                        f.frame >= m.startup + 2 && moves::block_cancel(f.action, candidate)
                    } else {
                        f.frame >= m.startup + m.active && moves::string(f.action, candidate)
                    }
                });
                // Pressure: the forward dash turns into an attack (or a grab).
                let dash_in = f.action == 3
                    && f.style == moves::PRESSURE
                    && f.dash_dir == f.facing
                    && f.frame >= moves::DASH_CANCEL
                    && !matches!(candidate, 0 | 3);
                // A landed uppercut can be chased: up after its active frames
                // jumps after the launched opponent (an air kick follows).
                let chase = f.action == 10
                    && f.confirmed
                    && buffered & JUMP != 0
                    && f.attack().is_some_and(|m| f.frame >= m.startup + m.active);
                if cancel || dash_in || chase || f.frame >= f.duration(f.action) {
                    f.action = 0;
                    f.frame = 0;
                }
            }
            let was_guard = f.guard;
            f.crouch = f.y == 0 && f.action == 0 && input & CROUCH != 0;
            f.guard = f.action == 0 && f.y == 0 && input & BLOCK != 0;
            if was_guard && !f.guard {
                f.parry_cooldown = PARRY_COOLDOWN;
            }
            f.guard_age = if !f.guard {
                0
            } else if was_guard {
                f.guard_age + 1
            } else if f.parry_cooldown > 0 {
                // Raised again too soon: a plain block, no parry window.
                PARRY
            } else {
                0
            };
            if f.action == 0 {
                let movement = i32::from(input & RIGHT != 0) - i32::from(input & LEFT != 0);
                if f.y == 0 {
                    // Walking at the style's pace, guarded or crouched slowly;
                    // RUN held runs forward (stamina) or backs off quicker.
                    let (forward, back) = moves::walk(f.style);
                    let run = input & RUN != 0 && !f.guard && !f.crouch && movement != 0 && f.stamina > 0;
                    f.running = run && movement == f.facing;
                    f.x += movement
                        * if f.guard || f.crouch {
                            WALK_SLOW
                        } else if movement == f.facing {
                            if run {
                                moves::run(f.style)
                            } else {
                                forward
                            }
                        } else if run {
                            back * 3 / 2
                        } else {
                            back
                        };
                    if f.running {
                        f.stamina = (f.stamina - RUN_COST).max(0);
                    }
                }
                if buffered & JUMP != 0 && f.y == 0 {
                    f.vy = 112;
                    f.y = 1;
                    // A running jump carries further.
                    f.vx = movement * if f.running || was_running { 60 } else { 42 };
                    f.running = false;
                    f.guard = false;
                    f.crouch = false;
                    f.air_attack = false;
                    f.buffer_time = 0;
                } else if candidate != 0 {
                    let cost = moves::attack_for(f.style, candidate).map_or(160, |m| m.cost);
                    let meter = matches!(candidate, 14 | 27);
                    if f.stamina >= cost
                        && (!meter || f.meter >= 500)
                        && (f.y == 0 || (moves::airborne(candidate) && !f.air_attack))
                    {
                        f.stamina -= cost;
                        if meter {
                            f.meter -= 500;
                        }
                        // A running fighter carries its speed into the blow.
                        if f.running || was_running {
                            f.vx = f.facing * RUN_CARRY;
                            f.running = false;
                        }
                        // Acting ends the wake-up (or breaker) protection.
                        f.invulnerable = 0;
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
                if !f.running {
                    f.stamina = (f.stamina + if f.guard { 3 } else { 6 }).min(1000);
                }
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
            self.freeze = STOP_GRAB;
        }
        if let Some(victim) = tech {
            self.break_throw(victim);
        }
        if let Some(victim) = slam {
            // The throw's damage lands with the body.
            let thrower = &mut self.fighters[1 - victim];
            let dealt = moves::attack_for(thrower.style, 4).map_or(0, |m| m.damage);
            thrower.meter = (thrower.meter + dealt * 6).min(1000);
            thrower.combo_damage = dealt;
            let f = &mut self.fighters[victim];
            f.hp = (f.hp - dealt).max(0);
            f.meter = (f.meter + dealt * 8).min(1000);
            self.event += 1;
            self.event_kind = 10;
            self.event_target = victim;
            self.freeze = 4;
        }
        // Pushboxes: fighters cannot cross; facing and control direction stay
        // predictable on phones.
        let push = boxes::PUSH_HALF;
        if self.fighters[1].x - self.fighters[0].x < 2 * push {
            let mid = ((self.fighters[0].x + self.fighters[1].x) / 2).clamp(-ARENA_LIMIT + push, ARENA_LIMIT - push);
            self.fighters[0].x = mid - push;
            self.fighters[1].x = mid + push;
        }
        // Resolve both intents from the same pre-hit state: simultaneous hits trade.
        let before = self.fighters.clone();
        let mut clash = false;
        for side in 0..2 {
            let a = &before[side];
            let d = &before[1 - side];
            let Some(m) = a.attack() else { continue };
            let Some(hit) = boxes::hitbox(a, &m) else { continue };
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
            // Only the back dash slips through attacks; the forward dash is
            // a committed approach.
            let backdash = d.action == 3 && d.dash_dir != d.facing && (2..=8).contains(&d.frame);
            if a.connected || clash || d.invulnerable > 0 || d.down > 0 || d.juggle >= 4 || backdash {
                continue;
            }
            // Contact: the blow's box meets the body (a grab) or the body or a
            // stretched limb (a strike). A crouching body ducks under a jab, a
            // jump clears a sweep, a whiffed kick can be hit on the leg.
            let d_move = d.attack();
            let hurt = boxes::hurtboxes(d, d_move.as_ref());
            let grab = m.height == Height::Grab;
            let touches = hurt.iter().take(if grab { 1 } else { 2 }).flatten().any(|b| hit.overlaps(b));
            // The rising uppercut also slips air attacks.
            let rising = d.action == 10 && d_move.is_some_and(|u| d.frame >= 1 && d.frame < u.startup + u.active);
            if !touches
                // No throws on a body in the air, reeling or blocking.
                || (grab && (d.y > 0 || d.stun > 0 || d.blockstun > 0 || d.held > 0))
                || (rising && a.y > 0)
            {
                continue;
            }
            // Two grabs at once break each other.
            if m.height == Height::Grab && d.action == 4 && d.frame >= m.startup && d.frame < m.startup + m.active {
                clash = true;
                self.break_throw(1 - side);
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
                if d.guard_age < PARRY && d.blockstun == 0 {
                    self.event_kind = 3;
                    self.fighters[side].stun = 24;
                    self.fighters[side].action = 5;
                    self.fighters[side].frame = 0;
                    self.fighters[1 - side].meter = (d.meter + 100).min(1000);
                    self.freeze = STOP_PARRY;
                } else {
                    let defender = &mut self.fighters[1 - side];
                    // Blocking costs stamina: long pressure can still break
                    // the guard, but not two blocked strings.
                    defender.stamina = (defender.stamina - m.damage * 10).max(0);
                    defender.blockstun = m.blockstun;
                    defender.vx = a.facing * (m.push / 4).max(18);
                    self.event_kind = 2;
                    if defender.stamina == 0 {
                        self.event_kind = 4;
                        defender.stun = 42;
                        defender.blockstun = 0;
                        defender.guard = false;
                        defender.action = 5;
                    }
                    self.freeze = STOP_BLOCK;
                }
            } else {
                // Chains are true only while the victim is still in hitstun/airborne.
                let chain = if d.stun > 0 || (d.y > 0 && d.juggle > 0) {
                    a.combo.min(5)
                } else {
                    0
                };
                let counter = d_move.is_some_and(|m| d.frame < m.startup);
                let punish = d_move.is_some_and(|m| d.frame >= m.startup + m.active);
                // A counter-hit kick or hook on a standing body: it reels.
                let crumple = counter && d.y == 0 && matches!(a.action, 8 | 17 | 18);
                let dealt = (m.damage + if counter { 3 } else { 0 })
                    * (100 - chain as i32 * 13).max(35)
                    / 100;
                let grab = a.action == 4;
                let defender = &mut self.fighters[1 - side];
                if !grab {
                    defender.hp = (defender.hp - dealt).max(0);
                    defender.meter = (defender.meter + dealt * 8).min(1000);
                }
                // A counter hit stuns longer: room for a bigger follow-up.
                defender.stun = if crumple {
                    CRUMPLE
                } else {
                    m.stun + if counter { COUNTER_STUN } else { 0 }
                };
                defender.action = 5;
                defender.frame = 0;
                defender.guard = false;
                defender.crouch = false;
                defender.blockstun = 0;
                // A reeling body stays where it was struck.
                defender.vx = a.facing * if crumple { 8 } else { m.push };
                defender.recoil_v = if m.heavy() { 150 } else { 80 };
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
                if grab {
                    // Grabbed: held in front of the thrower, then slammed
                    // (damage on the slam). Only presses after the grab
                    // count for breaking it.
                    defender.held = HOLD;
                    defender.stun = 0;
                    defender.down = 0;
                    defender.juggle = 0;
                    defender.vx = 0;
                    defender.vy = 0;
                    defender.y = 0;
                    defender.buffer_time = 0;
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
                if !grab {
                    attacker.meter = (attacker.meter + dealt * 6).min(1000);
                }
                self.event_kind = if grab {
                    5
                } else if crumple {
                    11
                } else if punish {
                    7
                } else if counter {
                    6
                } else {
                    1
                };
                self.freeze = if grab {
                    STOP_GRAB
                } else if m.heavy() {
                    STOP_HEAVY
                } else {
                    STOP_LIGHT
                } + if counter { STOP_COUNTER } else { 0 };
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
                    } else if moves::airborne(f.action) {
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
/// Integer hash for the bot's plans (deterministic, no state).
fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}
fn select_action(f: &Fighter, bits: u32) -> u32 {
    if bits == 0 {
        return 0;
    }
    if bits & DASH != 0 {
        return 3;
    }
    // Held toward or away from the opponent: the command moves (only from a
    // standstill or a walk; strings keep their own order).
    let toward = if f.facing > 0 { RIGHT } else { LEFT };
    let away = if f.facing > 0 { LEFT } else { RIGHT };
    let ground = f.y == 0 && f.action == 0 && bits & CROUCH == 0;
    let forward = ground && bits & toward != 0 && bits & away == 0;
    let back = ground && bits & away != 0 && bits & toward == 0;
    if bits & SPECIAL != 0 {
        return if forward { 27 } else { 14 };
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
        } else if forward {
            24
        } else if back {
            25
        } else {
            match f.action {
                11 | 18 => 12,
                8 => 18,
                _ => 8,
            }
        };
    }
    if bits & HEAVY != 0 {
        return if f.y > 0 {
            21
        } else if bits & CROUCH != 0 || matches!(f.action, 8 | 11 | 17) {
            10
        } else if forward {
            26
        } else {
            2
        };
    }
    if bits & LIGHT != 0 {
        return if f.y > 0 {
            20
        } else if bits & CROUCH != 0 && f.action != 1 && f.action != 11 {
            16
        } else if forward {
            22
        } else if back {
            23
        } else {
            match f.action {
                1 => 11,
                11 => 17,
                _ => 1,
            }
        };
    }
    0
}
/// Ticks `action` lasts in the all-round style (`Fighter::duration` for a fighter's own).
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
        static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }
    /// A buffer of `len` bytes for `arena_load` to read a state from.
    #[no_mangle]
    pub extern "C" fn arena_alloc(len: usize) -> *mut u8 {
        INPUT.with(|b| {
            let mut b = b.borrow_mut();
            b.clear();
            b.resize(len, 0);
            b.as_mut_ptr()
        })
    }
    /// Replaces the match with the JSON state in the `arena_alloc` buffer
    /// (client prediction rolls back to each server snapshot). 1 on success.
    #[no_mangle]
    pub extern "C" fn arena_load(len: usize) -> u32 {
        INPUT.with(|b| {
            let b = b.borrow();
            let state = std::str::from_utf8(&b[..len.min(b.len())]).ok().and_then(|s| Match::deserialize_json(s).ok());
            match state {
                Some(m) => {
                    GAME.with(|g| *g.borrow_mut() = m);
                    1
                }
                None => 0,
            }
        })
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
    /// A side's fighting style (0 all-round, 1 pressure, 2 range).
    #[no_mangle]
    pub extern "C" fn arena_style(side: u32, style: u32) {
        GAME.with(|g| g.borrow_mut().set_style(side as usize, style));
    }
    /// The simulation tick (cheap, without serialising the state).
    #[no_mangle]
    pub extern "C" fn arena_tick() -> u32 {
        GAME.with(|g| g.borrow().tick)
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
        assert_eq!(m.fighters[1].hp, 100 - attack(1).unwrap().damage);
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
        run(&mut m, [LEFT, RIGHT], 1300);
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
        run(&mut m, [LEFT, RIGHT], 1300);
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
        assert_eq!(m.fighters[1].held > 0, true, "a held guard does not stop a grab");
        run(&mut m, [0, BLOCK], 50);
        assert_eq!(m.fighters[1].hp, 100 - attack(4).unwrap().damage, "the throw's damage lands with the slam");
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
        assert_eq!(m.fighters[1].hp, 100 - attack(4).unwrap().damage);
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
        assert_eq!(m.fighters[0].hp, 100 - attack(1).unwrap().damage);
        assert_eq!(m.fighters[1].hp, 100 - attack(1).unwrap().damage);
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
        run(&mut m, [RIGHT, LEFT], 1000);
        assert!(m.fighters[1].x - m.fighters[0].x >= 600);
        run(&mut m, [LEFT, RIGHT], 1300);
        assert_eq!(m.fighters[0].x, -ARENA_LIMIT);
        assert_eq!(m.fighters[1].x, ARENA_LIMIT);
    }
    #[test]
    fn high_low_overhead_defences_have_real_tradeoffs() {
        let mut standing = duel();
        run(&mut standing, [0, BLOCK], 10);
        run(&mut standing, [CROUCH | KICK, BLOCK], 25);
        assert_eq!(standing.fighters[1].hp, 100 - attack(9).unwrap().damage);
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
        assert_eq!(m.fighters[1].hp, 100 - attack(1).unwrap().damage);
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
        let full: i32 = [1, 11, 12].iter().map(|&a| attack(a).unwrap().damage).sum();
        assert!(m.fighters[0].combo_damage < full);
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
    /// Can `side` start a fresh attack on the next tick? HEAVY has no
    /// string follow-up, so only a finished recovery lets it start.
    fn can_start(m: &Match, side: usize, guard: u32) -> bool {
        let mut trial = m.clone();
        trial.fighters[side].previous = 0;
        trial.fighters[side].connected = false;
        trial.fighters[side].confirmed = false;
        let mut inputs = [guard, guard];
        inputs[side] = HEAVY;
        trial.step(inputs);
        attack(trial.fighters[side].action).is_some() && trial.fighters[side].frame == 0
    }
    /// Frame advantage of the attacker after `action` is blocked at 900 mm:
    /// first tick the defender can act minus the attacker's (§3.3 of
    /// docs/COMBAT_RESEARCH.md).
    fn block_advantage(action: u32) -> i32 {
        let mut m = duel();
        m.fighters[0].x = -450;
        m.fighters[1].x = 450;
        m.fighters[0].action = action;
        m.fighters[1].guard = true;
        m.fighters[1].guard_age = 20;
        let guard = BLOCK | if attack(action).unwrap().height == Height::Low { CROUCH } else { 0 };
        let mut contact = 0;
        let mut ready = [0i32; 2];
        for t in 1..150 {
            m.step([0, guard]);
            if contact == 0 && m.event_kind == 2 {
                contact = t;
            }
            if contact > 0 {
                for side in 0..2 {
                    if ready[side] == 0 && can_start(&m, side, if side == 1 { guard } else { 0 }) {
                        ready[side] = t + 1;
                    }
                }
                if ready[0] > 0 && ready[1] > 0 {
                    break;
                }
            }
        }
        assert!(contact > 0, "action {action} was not blocked");
        ready[1] - ready[0]
    }
    #[test]
    fn block_advantage_matches_the_design() {
        let mut report = String::new();
        let mut adv = |action: u32| {
            let a = block_advantage(action);
            report += &format!("{action}:{a} ");
            a
        };
        // Light pressure tools are slightly minus, enders are punishable.
        let (jab, low, cross, kick) = (adv(1), adv(16), adv(11), adv(8));
        let (special, heavy, sweep, uppercut, roundhouse) = (adv(14), adv(2), adv(9), adv(10), adv(12));
        let (hook, side_kick) = (adv(17), adv(18));
        let (lunge, rear_uppercut, advancing, thrust) = (adv(22), adv(23), adv(24), adv(25));
        let (knee, hurricane) = (adv(26), adv(27));
        eprintln!("block advantage {report}");
        assert!((-2..=0).contains(&jab), "jab {jab}");
        assert!((-4..=-2).contains(&low), "low kick {low}");
        assert!((-5..=-3).contains(&cross), "cross {cross}");
        assert!((-5..=-3).contains(&kick), "kick {kick}");
        assert!((-3..=0).contains(&special), "special {special}");
        assert!(heavy <= -10, "heavy {heavy}");
        assert!(sweep <= -12, "sweep {sweep}");
        assert!(uppercut <= -16, "uppercut {uppercut}");
        assert!(roundhouse <= -8, "roundhouse {roundhouse}");
        assert!((-6..=-4).contains(&hook), "hook {hook}");
        assert!((-6..=-3).contains(&side_kick), "side kick {side_kick}");
        // Command moves: the lunging hook keeps the initiative, the thrust kick is
        // safe, the advancing roundhouse a little minus, the flying knee and
        // the hurricane kick are punishable.
        assert!((0..=2).contains(&lunge), "lunging hook {lunge}");
        assert!(rear_uppercut <= -6, "rear uppercut {rear_uppercut}");
        assert!((-8..=-4).contains(&advancing), "advancing roundhouse {advancing}");
        assert!((-4..=0).contains(&thrust), "thrust kick {thrust}");
        assert!(knee <= -12, "flying knee {knee}");
        assert!(hurricane <= -8, "hurricane {hurricane}");
    }
    #[test]
    fn running_covers_ground_and_carries_into_a_blow() {
        let travel = |input: u32| {
            let mut m = duel();
            m.fighters[0].x = -6000;
            let x = m.fighters[0].x;
            run(&mut m, [input, 0], 30);
            (m.fighters[0].x - x, m.fighters[0].stamina)
        };
        let (walked, _) = travel(RIGHT);
        let (ran, stamina) = travel(RIGHT | RUN);
        assert_eq!(walked, 30 * moves::walk(0).0);
        assert_eq!(ran, 30 * moves::run(0));
        assert_eq!(stamina, 1000 - 30 * RUN_COST, "running costs stamina");
        let (backed, _) = travel(LEFT | RUN);
        assert_eq!(backed, -30 * moves::walk(0).1 * 3 / 2);
        // A jab out of a run slides in with it; a guard stops the run.
        let mut m = duel();
        m.fighters[0].x = -3000;
        run(&mut m, [RIGHT | RUN, 0], 10);
        assert!(m.fighters[0].running);
        m.step([RIGHT | RUN | LIGHT, 0]);
        assert_eq!(m.fighters[0].action, 22, "forward + punch while running: the lunging hook");
        assert!(m.fighters[0].vx > 0);
        let mut guarded = duel();
        guarded.fighters[0].x = -3000;
        run(&mut guarded, [RIGHT | RUN | BLOCK, 0], 10);
        assert!(!guarded.fighters[0].running);
    }
    #[test]
    fn direction_and_button_pick_the_command_moves() {
        let first = |input: u32, meter: i32| {
            let mut m = duel();
            m.fighters[0].meter = meter;
            m.step([input, 0]);
            m.fighters[0].action
        };
        assert_eq!(first(LIGHT, 0), 1);
        assert_eq!(first(RIGHT | LIGHT, 0), 22);
        assert_eq!(first(LEFT | LIGHT, 0), 23);
        assert_eq!(first(RIGHT | KICK, 0), 24);
        assert_eq!(first(LEFT | KICK, 0), 25);
        assert_eq!(first(RIGHT | HEAVY, 0), 26);
        assert_eq!(first(LEFT | HEAVY, 0), 2);
        assert_eq!(first(RIGHT | SPECIAL, 600), 27);
        assert_eq!(first(RIGHT | SPECIAL, 100), 0, "no meter, no hurricane");
        assert_eq!(first(RIGHT | CROUCH | LIGHT, 0), 16, "crouching keeps the low set");
        // The right-hand fighter faces left: its forward is LEFT.
        let mut m = duel();
        m.step([0, LEFT | KICK]);
        assert_eq!(m.fighters[1].action, 24);
        // A string keeps its order whatever the stick says.
        let mut s = duel();
        run(&mut s, [LIGHT, 0], 2);
        run(&mut s, [0, 0], 7);
        run(&mut s, [RIGHT | LIGHT, 0], 3);
        run(&mut s, [RIGHT, 0], 12);
        assert_eq!(s.fighters[0].action, 11);
        // The advancing roundhouse and the flying knee carry the body.
        let mut a = duel();
        a.fighters[0].x = -3000;
        a.fighters[1].x = 3000;
        run(&mut a, [RIGHT | KICK, 0], 20);
        assert!(a.fighters[0].x > -3000 + 300, "advanced {}", a.fighters[0].x + 3000);
    }
    #[test]
    fn each_button_has_its_jump_attack_once_per_jump() {
        for (button, action) in [(LIGHT, 20), (KICK, 13), (HEAVY, 21)] {
            let mut m = duel();
            m.fighters[0].x = -3000;
            m.step([JUMP | RIGHT, 0]);
            run(&mut m, [0, 0], 6);
            m.step([button, 0]);
            m.step([0, 0]);
            assert_eq!(m.fighters[0].action, action);
            run(&mut m, [0, 0], 30);
            m.step([button, 0]);
            let f = &m.fighters[0];
            assert!(f.y > 0 && !(moves::airborne(f.action) && f.frame <= 1), "a second jump attack in the same jump");
            run(&mut m, [0, 0], 60);
            assert_eq!((m.fighters[0].y, m.fighters[0].action), (0, 0), "lands out of it");
        }
        // The bicycle kick knocks a standing opponent down.
        let mut m = duel();
        m.fighters[0].y = 600;
        m.fighters[0].vy = -10;
        m.fighters[0].action = 21;
        m.fighters[0].air_attack = true;
        run(&mut m, [0, 0], 20);
        assert!(m.fighters[1].hp < 100 && m.fighters[1].down > 0, "knocked down");
    }
    #[test]
    fn low_kick_opens_a_standing_guard_and_links_into_the_uppercut() {
        let mut standing = duel();
        run(&mut standing, [0, BLOCK], 10);
        run(&mut standing, [CROUCH | LIGHT, BLOCK], 12);
        assert_eq!(standing.fighters[0].action, 16);
        assert!(standing.fighters[1].hp < 100, "a standing guard does not stop a low");
        assert_eq!(standing.fighters[1].down, 0, "the low kick does not knock down");
        let mut low = duel();
        run(&mut low, [0, CROUCH | BLOCK], 10);
        run(&mut low, [CROUCH | LIGHT, CROUCH | BLOCK], 20);
        assert_eq!(low.fighters[1].hp, 100);
        assert_eq!(low.event_kind, 2);
        // On hit the low kick cancels into the rising uppercut, which launches.
        let mut combo = duel();
        run(&mut combo, [CROUCH | LIGHT, 0], 10);
        run(&mut combo, [CROUCH | HEAVY, 0], 20);
        assert_eq!(combo.fighters[0].combo, 2);
        assert!(combo.fighters[1].y > 0 && combo.fighters[1].juggle > 0, "launched");
    }
    #[test]
    fn rising_uppercut_beats_jump_ins_and_high_attacks() {
        // A jump-in air kick meets the uppercut: the jumper is launched.
        let mut m = duel();
        m.fighters[0].x = -500;
        m.fighters[1].x = 500;
        m.fighters[0].y = 700;
        m.fighters[0].vy = -10;
        m.fighters[0].action = 13;
        m.fighters[0].air_attack = true;
        m.fighters[1].action = 10;
        m.fighters[1].frame = 4;
        run(&mut m, [0, 0], 14);
        assert_eq!(m.fighters[1].hp, 100, "the uppercut slips the air kick");
        assert!(m.fighters[0].hp < 100 && m.fighters[0].juggle > 0, "the jumper is launched");
        // A jab into the rising uppercut whiffs; a mid kick does not.
        let mut jab = duel();
        jab.step([0, CROUCH | HEAVY]);
        run(&mut jab, [LIGHT, 0], 20);
        assert_eq!(jab.fighters[1].hp, 100);
        assert!(jab.fighters[0].hp < 100);
    }
    #[test]
    fn juggled_fighter_cannot_attack_before_landing() {
        let mut m = duel();
        m.fighters[1].y = 900;
        m.fighters[1].vy = 20;
        m.fighters[1].juggle = 1;
        m.fighters[1].stun = 5;
        for _ in 0..80 {
            m.step([0, KICK]);
            m.step([0, 0]);
            assert_ne!(m.fighters[1].action, 13, "air kick out of a juggle");
            if m.fighters[1].down > 0 {
                return;
            }
        }
        panic!("never landed");
    }
    #[test]
    fn throws_break_on_a_quick_tech_and_never_hit_a_blocking_fighter() {
        // THROW right after the grab: both stagger apart, nobody is hurt.
        let mut m = duel();
        run(&mut m, [THROW, 0], 16);
        assert!(m.fighters[1].held > 0);
        m.step([0, THROW]);
        run(&mut m, [0, 0], 60);
        assert_eq!(m.event_kind, 9);
        assert_eq!(m.fighters[1].hp, 100);
        assert_eq!(m.fighters[1].down, 0);
        // Too late: the grip holds.
        let mut late = duel();
        run(&mut late, [THROW, 0], 16);
        run(&mut late, [0, 0], 20);
        late.step([0, THROW]);
        run(&mut late, [0, 0], 60);
        assert_eq!(late.fighters[1].hp, 100 - attack(4).unwrap().damage);
        // A defender in blockstun cannot be grabbed.
        let mut pressure = duel();
        pressure.fighters[1].guard = true;
        pressure.fighters[1].guard_age = 20;
        pressure.fighters[1].blockstun = 12;
        pressure.fighters[0].action = 4;
        pressure.fighters[0].frame = 12;
        run(&mut pressure, [0, BLOCK], 3);
        assert_eq!(pressure.fighters[1].held, 0);
    }
    #[test]
    fn tapping_block_does_not_parry_but_a_fresh_guard_does() {
        let mut tapped = duel();
        run(&mut tapped, [0, BLOCK], 4);
        run(&mut tapped, [0, 0], 3);
        run(&mut tapped, [LIGHT, 0], 4);
        run(&mut tapped, [0, BLOCK], 8);
        assert_eq!(tapped.event_kind, 2, "re-raised guard only blocks");
        let mut fresh = duel();
        run(&mut fresh, [LIGHT, 0], 4);
        run(&mut fresh, [0, BLOCK], 8);
        assert_eq!(fresh.event_kind, 3);
    }
    #[test]
    fn counter_hits_reward_the_read() {
        // A kick into the startup of a heavy: the victim reels long enough
        // for a free string.
        let mut m = duel();
        m.step([0, HEAVY]);
        let mut reeling = false;
        for _ in 0..30 {
            m.step([KICK, 0]);
            m.step([0, 0]);
            if m.event_kind == 11 {
                reeling = true;
                break;
            }
        }
        assert!(reeling, "counter-hit kick crumples");
        assert!(m.fighters[1].stun > COUNTER_STUN + 20);
        let hp = m.fighters[1].hp;
        for t in 0..60 {
            m.step([if t % 10 < 3 { LIGHT } else { 0 }, LIGHT]);
        }
        assert!(m.fighters[0].combo >= 3, "the reel is comboed: {}", m.fighters[0].combo);
        assert!(m.fighters[1].hp < hp);
        // A jab into the heavy is a plain counter: a few ticks of extra stun.
        let mut jab = duel();
        jab.step([0, HEAVY]);
        run(&mut jab, [LIGHT, 0], 9);
        assert_eq!(jab.event_kind, 6);
        let stun = attack(1).unwrap().stun + COUNTER_STUN;
        assert!(jab.fighters[1].stun <= stun && jab.fighters[1].stun > stun - 10);
    }
    #[test]
    fn a_landed_uppercut_is_chased_into_the_air() {
        let mut m = duel();
        run(&mut m, [CROUCH | LIGHT, 0], 10);
        run(&mut m, [CROUCH | HEAVY, 0], 20);
        assert!(m.fighters[1].juggle == 1 && m.fighters[1].y > 0, "launched");
        // Up-forward once the uppercut's active frames are over: a jump.
        let mut jumped = 0;
        for _ in 0..30 {
            m.step([JUMP | RIGHT, 0]);
            m.step([0, 0]);
            if m.fighters[0].y > 0 {
                jumped = m.tick;
                break;
            }
        }
        assert!(jumped > 0, "the uppercut jump-cancels on hit");
        run(&mut m, [0, 0], 4);
        let hp = m.fighters[1].hp;
        run(&mut m, [KICK, 0], 2);
        run(&mut m, [0, 0], 20);
        assert!(m.fighters[1].hp < hp, "the air kick connects");
        assert_eq!(m.fighters[0].combo, 3);
        // A blocked or whiffed uppercut cannot be jumped out of.
        let mut whiff = duel();
        whiff.fighters[1].x = 2500;
        run(&mut whiff, [CROUCH | HEAVY, 0], 18);
        run(&mut whiff, [JUMP, 0], 2);
        assert_eq!(whiff.fighters[0].y, 0);
        assert_eq!(whiff.fighters[0].action, 10);
    }
    #[test]
    fn up_on_the_floor_gets_up_quicker_without_the_protection() {
        let rise = |press: bool| {
            let mut m = duel();
            m.fighters[0].down = KNOCKDOWN;
            m.fighters[0].action = 15;
            for t in 0..KNOCKDOWN {
                let up = press && m.fighters[0].frame == QUICK_RISE_FROM + 2;
                m.step([if up { JUMP } else { 0 }, 0]);
                if m.fighters[0].down == 0 {
                    return (t + 1, m.fighters[0].invulnerable, m.fighters[0].y);
                }
            }
            panic!("still down");
        };
        let (normal, guard, _) = rise(false);
        let (quick, quick_guard, y) = rise(true);
        assert_eq!(normal, KNOCKDOWN);
        assert!(quick + 12 < normal, "quick {quick} vs {normal}");
        assert_eq!((guard, quick_guard), (WAKE_GUARD, QUICK_WAKE_GUARD));
        assert_eq!(y, 0, "the press is spent on the rise, not a jump");
    }
    #[test]
    fn attacking_on_wakeup_drops_the_protection() {
        let mut m = duel();
        m.fighters[0].down = 1;
        m.step([0, 0]);
        assert!(m.fighters[0].invulnerable > 0);
        m.step([LIGHT, 0]);
        assert_eq!(m.fighters[0].action, 1);
        assert_eq!(m.fighters[0].invulnerable, 0);
    }
    #[test]
    fn forward_dash_is_a_commitment_back_dash_evades() {
        let mut forward = duel();
        forward.fighters[0].x = -700;
        forward.fighters[1].x = 700;
        run(&mut forward, [LIGHT, DASH | LEFT], 14);
        assert!(forward.fighters[1].hp < 100, "dashing in is hit");
        let mut back = duel();
        run(&mut back, [LIGHT, DASH], 14);
        assert_eq!(back.fighters[1].hp, 100);
    }
    #[test]
    fn repeated_buttons_play_strings_even_on_a_whiff() {
        let mash = |button: u32| {
            let mut m = Match::default();
            m.phase = 1;
            let mut seen = vec![];
            for t in 0..150 {
                m.step([if t % 6 < 3 { button } else { 0 }, 0]);
                let a = m.fighters[0].action;
                if a != 0 && seen.last() != Some(&a) {
                    seen.push(a);
                }
            }
            seen
        };
        let punches = mash(LIGHT);
        assert!(punches.starts_with(&[1, 11, 17, 1]), "J-J-J-J: {punches:?}");
        let kicks = mash(KICK);
        // The roundhouse after a whiffed side kick only starts once it recovers.
        assert!(kicks.starts_with(&[8, 18, 12, 8]), "U-U-U-U: {kicks:?}");
    }
    #[test]
    fn light_strings_continue_on_block() {
        let mut m = duel();
        run(&mut m, [0, BLOCK], 10);
        run(&mut m, [LIGHT, BLOCK], 11);
        run(&mut m, [0, BLOCK], 1);
        run(&mut m, [LIGHT, BLOCK], 3);
        assert_eq!(m.fighters[0].action, 11, "jab -> cross on block");
        // Heavy enders do not cancel from a blocked jab.
        let mut heavy = duel();
        run(&mut heavy, [0, BLOCK], 10);
        run(&mut heavy, [LIGHT, BLOCK], 11);
        run(&mut heavy, [HEAVY, BLOCK], 3);
        assert_eq!(heavy.fighters[0].action, 1);
    }
    #[test]
    fn a_reloaded_snapshot_continues_exactly_like_the_original() {
        // Client prediction reloads server snapshots and replays from them:
        // the JSON round trip must keep every competitive field.
        let mut a = Match::new(91);
        for t in 0..20_000 {
            let inputs = [a.bot_input(0), a.bot_input(1)];
            if t % 97 == 0 {
                let mut b = Match::deserialize_json(&a.serialize_json()).unwrap();
                let mut c = a.clone();
                for k in 0..120u32 {
                    let i = [c.bot_input(0) ^ (k % 7), c.bot_input(1)];
                    c.step(i);
                    b.step(i);
                }
                assert_eq!(b.serialize_json(), c.serialize_json(), "diverged after reload at tick {t}");
            }
            a.step(inputs);
        }
    }
    #[test]
    fn a_whiffed_kick_is_hit_on_the_leg() {
        // At 1.7 m a front kick (reach 1.64 m) whiffs and a jab (1.18 m)
        // cannot reach the kicker's body — but it reaches the stretched leg.
        let stage = |kick: bool| {
            let mut m = duel();
            m.fighters[0].x = -850;
            m.fighters[1].x = 850;
            if kick {
                m.fighters[0].action = 8;
            }
            // The jab comes after the kick's active frames, into its recovery
            // (a jab into the active kick would lose its arm to it).
            run(&mut m, [0, 0], 10);
            run(&mut m, [0, LIGHT], 10);
            m
        };
        let punished = stage(true);
        assert!(punished.fighters[0].hp < 100, "the leg was hit");
        assert_eq!(punished.fighters[1].hp, 100);
        let idle = stage(false);
        assert_eq!(idle.fighters[0].hp, 100, "the body is out of jab range");
    }
    #[test]
    fn crouching_ducks_the_jab_guard_or_not_and_a_jump_clears_lows() {
        for guard in [0, BLOCK] {
            let mut m = duel();
            run(&mut m, [0, CROUCH | guard], 4);
            run(&mut m, [LIGHT, CROUCH | guard], 16);
            assert_eq!(m.fighters[1].hp, 100);
            assert_eq!(m.event, 0, "no hit, no block: the jab passes over");
        }
        // A cross (mid) hits the crouching head.
        let mut m = duel();
        run(&mut m, [0, CROUCH], 4);
        m.fighters[0].action = 11;
        run(&mut m, [0, CROUCH], 10);
        assert!(m.fighters[1].hp < 100);
        // A body 400 mm up is above a sweep (at the ankles).
        let mut air = duel();
        air.fighters[1].y = 400;
        air.fighters[1].vy = 30;
        air.fighters[0].action = 9;
        air.fighters[0].frame = 13;
        run(&mut air, [0, 0], 4);
        assert_eq!(air.fighters[1].hp, 100);
    }
    #[test]
    fn styles_change_reach_speed_and_survive_the_round() {
        let walked = |style: u32| {
            let mut m = duel();
            m.set_style(0, style);
            run(&mut m, [LEFT, 0], 60);
            -400 - m.fighters[0].x
        };
        assert!(walked(moves::RANGE) > walked(moves::ALLROUND), "range retreats faster");
        assert!(walked(moves::PRESSURE) < walked(moves::ALLROUND));
        // At 1.76 m only the range style's side kick (U-U) lands.
        let kick = |style: u32| {
            let mut m = duel();
            m.fighters[0].x = -880;
            m.fighters[1].x = 880;
            m.set_style(0, style);
            m.fighters[0].action = 18;
            run(&mut m, [0, 0], 20);
            m.fighters[1].hp
        };
        assert!(kick(moves::RANGE) < 100);
        assert_eq!(kick(moves::ALLROUND), 100);
        assert_eq!(kick(moves::PRESSURE), 100);
        // The pressure jab comes out a tick sooner.
        assert_eq!(moves::attack_for(moves::PRESSURE, 1).unwrap().startup + 1, attack(1).unwrap().startup);
        // Styles stay with the fighters into the next round.
        let mut m = duel();
        m.set_style(0, moves::PRESSURE);
        m.set_style(1, moves::RANGE);
        m.remaining = 1;
        m.step([0, 0]);
        run(&mut m, [0, 0], 270);
        assert_eq!(m.round, 2);
        assert_eq!([m.fighters[0].style, m.fighters[1].style], [moves::PRESSURE, moves::RANGE]);
    }
    #[test]
    fn styles_are_close_in_bot_play() {
        // Not a proof of fairness: the bot plays every matchup; no style may
        // run away with it.
        let mut wins = [[0u32; 3]; 3];
        for a in 0..3u32 {
            for b in 0..3u32 {
                for seed in 1..=24u32 {
                    let mut m = Match::new(seed * 7919 + a * 31 + b * 7);
                    m.set_style(0, a);
                    m.set_style(1, b);
                    for _ in 0..60 * 60 * 4 {
                        if m.phase == 3 {
                            break;
                        }
                        let inputs = [m.bot_input(0), m.bot_input(1)];
                        m.step(inputs);
                    }
                    if m.winner == 0 {
                        wins[a as usize][b as usize] += 1;
                    }
                }
            }
        }
        eprintln!("left wins of 24 (rows: left style, columns: right style): {wins:?}");
        for a in 0..3 {
            for b in 0..3 {
                assert!((4..=20).contains(&wins[a][b]), "style {a} vs {b}: {} of 24", wins[a][b]);
            }
        }
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
