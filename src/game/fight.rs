//! Arena presentation on the existing miniquad renderer. Combat lives in arena-combat.
//!
//! Snapshots arrive at the simulation's pace (60 Hz locally, 20 Hz online);
//! everything visual is evaluated in `draw` at the display's own refresh rate
//! from a continuous render tick, so motion stays smooth at 60, 90 or 120 Hz.
use super::{
    anims::Reaction,
    arena_props::ArenaProps,
    effects::Effects,
    scenery::Scenery,
    fighter_model::{FighterModel, View},
    ragdoll::Part,
    timeline::{Body, Timeline},
};
use crate::engine::{
    app::{FrameCtx, Scene, Transition},
    graphics::Graphics,
    math::*,
    math3::*,
    mesh3::MeshData,
    render3d::{Camera, Lighting, SkinnedMesh},
};
use arena_combat::{moves, Fighter, Match};
#[cfg(target_arch = "wasm32")]
use nanoserde::DeJson;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};
#[cfg(target_arch = "wasm32")]
extern "C" {
    fn fight_read(ptr: *mut u8, capacity: usize) -> usize;
    fn fight_model_status(status: i32);
    /// Screenshot hook: eye xyz, target xyz, fov, preview flag, then a
    /// captured clip index and time to review on the left fighter (or -1).
    fn fight_debug_camera(out: *mut f32) -> i32;
    /// Continuous render tick from the page (negative: show the newest state).
    fn fight_clock() -> f64;
    /// Screen area free of HUD and touch controls: top, bottom (0..1 from
    /// the top), left, right (0..1 from the left).
    fn fight_layout(out: *mut f32) -> i32;
    /// Fighter assets per side as UTF-8 lines: model, pack, model, pack.
    fn fight_fighters(ptr: *mut u8, capacity: usize) -> usize;
    /// Which sides show the requested fighter (bit 0 left, bit 1 right).
    fn fight_avatars(mask: i32);
    /// The baked room: 1 shown, -1 unavailable (box room stays).
    fn fight_room_status(status: i32);
    /// Debug flags from the page: 1 shows the hit, hurt and push boxes.
    fn fight_flags() -> i32;
    /// Largest room texture side, 0 for no limit.
    fn fight_texture_cap() -> i32;
    /// The baked room to load (UTF-8 path; 0: the default).
    fn fight_room_url(ptr: *mut u8, capacity: usize) -> usize;
}

/// The fighter everyone starts with; also the stand-in while others load.
const DEFAULT_MODEL: &str = "assets/character.glb";
const DEFAULT_PACK: &str = "assets/fight.pack";

type Pending = Rc<RefCell<Option<Result<Vec<u8>, String>>>>;

/// A fighter body requested by the page, by model path.
enum AvatarSlot {
    Loading { model: Pending, pack: Pending },
    Ready(usize),
    Failed,
}

fn fetch(path: &str) -> Pending {
    let slot: Pending = Rc::new(RefCell::new(None));
    let sink = slot.clone();
    crate::engine::assets::load(path, move |bytes| *sink.borrow_mut() = Some(bytes));
    slot
}

const FOV: f32 = 0.62;

/// Desktops get the full-resolution room, phones a lighter copy (the page decides).
fn room_url() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let mut bytes = [0u8; 256];
        let n = unsafe { fight_room_url(bytes.as_mut_ptr(), bytes.len()) };
        if let Some(path) = (n > 0 && n <= bytes.len()).then(|| std::str::from_utf8(&bytes[..n]).ok()).flatten() {
            return path.to_string();
        }
    }
    "assets/room.glb".to_string()
}

/// A combat event waiting for the render clock to reach its tick.
struct Event {
    tick: u32,
    kind: u32,
    target: usize,
    attacker_action: u32,
}

pub struct FightScene {
    timeline: Timeline,
    state: Match,
    meshes: Vec<SkinnedMesh>,
    model: Option<FighterModel>,
    loading: Rc<RefCell<Option<Result<Vec<u8>, String>>>>,
    /// Captured clips (optional): applied once the model is ready.
    pack: Rc<RefCell<Option<Result<Vec<u8>, String>>>>,
    /// Other fighters' bodies, fetched on demand.
    avatars: HashMap<String, AvatarSlot>,
    /// The baked apartment (assets/room.glb), handed to the props once loaded.
    room: Pending,
    /// Model and pack shown on each side.
    wanted: [(String, String); 2],
    /// Sides already showing the requested fighter, as last told to the page.
    avatars_ready: i32,
    props: ArenaProps,
    /// The city behind the windows and photos on screens (from the page).
    scenery: Scenery,
    effects: Effects,
    time: f32,
    last_draw: f64,
    last_event: u32,
    wall_impacts: [u32; 2],
    pending: VecDeque<Event>,
    flash: f32,
    impact: Vec3,
    impact_kind: u32,
    hit_flash: [f32; 2],
    /// Cyan rim on a fighter whose guard just took a blow.
    guard_flash: [f32; 2],
    shake: f32,
    punch: f32,
    view_distance: f32,
    view_center: f32,
    view_lift: f32,
    fighters: [Fighter; 2],
    bodies: [Body; 2],
    last_action: [(u32, f32); 2],
    reaction: [Reaction; 2],
    phase: u32,
    phase_time: f32,
    trails: [Vec<(Vec3, f32)>; 2],
    debug_camera: Option<[f32; 8]>,
    layout: [f32; 4],
    /// Seconds of knockout slow motion left, and who has been seen KO'd.
    slowmo: f32,
    /// The tick on screen. It follows the simulation's clock, except in the
    /// knockout's slow motion, when it advances slower (bodies fly slower
    /// too) and then catches up; it never runs backwards.
    view: f64,
    /// Knocked out this round (the slow motion plays once).
    knocked: [bool; 2],
    /// 0..1: the camera has moved in on the dancing winner.
    celebrate: f32,
}

impl FightScene {
    pub fn new() -> Self {
        let loading = Rc::new(RefCell::new(None));
        let result = loading.clone();
        crate::engine::assets::load("assets/character.glb", move |bytes| {
            *result.borrow_mut() = Some(bytes)
        });
        let pack = Rc::new(RefCell::new(None));
        let sink = pack.clone();
        crate::engine::assets::load("assets/fight.pack", move |bytes| {
            *sink.borrow_mut() = Some(bytes)
        });
        let state = Match::default();
        let mut timeline = Timeline::default();
        timeline.push(state.clone());
        Self {
            fighters: state.fighters.clone(),
            timeline,
            state,
            meshes: Vec::new(),
            model: None,
            loading,
            pack,
            avatars: HashMap::new(),
            room: fetch(&room_url()),
            wanted: [
                (DEFAULT_MODEL.to_string(), DEFAULT_PACK.to_string()),
                (DEFAULT_MODEL.to_string(), DEFAULT_PACK.to_string()),
            ],
            avatars_ready: -1,
            props: ArenaProps::new(),
            scenery: Scenery::new(),
            effects: Effects::new(),
            time: 0.0,
            last_draw: miniquad::date::now(),
            last_event: 0,
            wall_impacts: [0; 2],
            pending: VecDeque::new(),
            flash: 0.0,
            impact: Vec3::ZERO,
            impact_kind: 0,
            hit_flash: [0.0; 2],
            guard_flash: [0.0; 2],
            shake: 0.0,
            punch: 0.0,
            view_distance: 6.0,
            view_center: 0.0,
            view_lift: 0.0,
            bodies: [
                Body { x: -1.15, ..Body::default() },
                Body { x: 1.15, ..Body::default() },
            ],
            last_action: [(0, 0.0); 2],
            reaction: [Reaction::Head; 2],
            phase: 0,
            phase_time: 0.0,
            trails: [vec![], vec![]],
            debug_camera: None,
            layout: [0.16, 0.95, 0.0, 1.0],
            slowmo: 0.0,
            view: f64::NAN,
            knocked: [false; 2],
            celebrate: 0.0,
        }
    }

    /// Takes a new authoritative snapshot and queues its events.
    fn accept(&mut self, state: Match) {
        let reset = self.timeline.push(state.clone());
        if reset || self.state.round != state.round {
            self.view_center = (state.fighters[0].x + state.fighters[1].x) as f32 / 2000.0;
            self.view_distance = 5.0;
            self.props.reset();
            self.effects.clear();
            self.pending.clear();
            self.trails = [vec![], vec![]];
            self.wall_impacts = [0; 2];
            self.last_event = state.event;
            self.reaction = [Reaction::Head; 2];
            self.celebrate = 0.0;
            self.view = f64::NAN;
            self.knocked = [false; 2];
            if let Some(model) = &mut self.model {
                model.reset();
            }
        }
        if state.event != self.last_event {
            self.last_event = state.event;
            let target = state.event_target.min(1);
            self.pending.push_back(Event {
                tick: state.tick,
                kind: state.event_kind,
                target,
                attacker_action: state.fighters[1 - target].action,
            });
        }
        for wall in 0..2 {
            let impacts = state.walls[wall].impacts;
            if impacts > self.wall_impacts[wall] {
                let f = &state.fighters;
                let side = if wall == 0 {
                    usize::from(f[1].x < f[0].x)
                } else {
                    usize::from(f[1].x > f[0].x)
                };
                self.pending.push_back(Event {
                    tick: state.tick,
                    kind: 100 + wall as u32,
                    target: side,
                    attacker_action: 0,
                });
            }
            self.wall_impacts[wall] = impacts;
        }
        self.state = state;
    }

    fn fire(&mut self, e: Event) {
        let preview = self.preview();
        if e.kind >= 100 {
            // Body slammed into a side wall.
            self.reaction[e.target] = Reaction::Wall;
            let x = self.bodies[e.target].x + if e.kind == 100 { -0.25 } else { 0.25 };
            if !preview {
                self.effects.dust(vec3(x, 1.0, 0.0), 10, 0.9);
                self.effects.dust(vec3(x, 0.05, 0.0), 8, 1.0);
            }
            self.shake = self.shake.max(0.7);
            if let Some(model) = &mut self.model {
                model.push(e.target, Part::Chest, vec3(if e.kind == 100 { 1.0 } else { -1.0 }, 0.3, 0.0), 2.6);
            }
            return;
        }
        let target = e.target;
        match e.kind {
            3 => self.reaction[1 - target] = Reaction::Parried,
            4 => self.reaction[target] = Reaction::GuardBreak,
            8 => self.reaction[1 - target] = Reaction::Pushed,
            // A broken throw: both stagger apart.
            9 => self.reaction = [Reaction::Pushed; 2],
            2 => {}
            // The throw's slam: the body is already on the floor.
            10 => {
                if !preview {
                    self.shake = self.shake.max(0.8);
                    self.punch = 0.7;
                    self.hit_flash[target] = 1.0;
                }
                return;
            }
            // A counter-hit kick or hook: the body reels, dazed.
            11 => self.reaction[target] = Reaction::GuardBreak,
            _ => {
                // A counter hit spins the body round (it also stuns longer).
                let r = Reaction::from_attack(e.attacker_action);
                self.reaction[target] = if e.kind == 6 && r != Reaction::Low { Reaction::Spin } else { r };
            }
        }
        let victim = &self.fighters[target];
        let blow = moves::attack_for(self.fighters[1 - target].style, e.attacker_action);
        // Counter hits land like heavy blows whatever struck.
        let heavy = blow.is_some_and(|m| m.heavy()) || matches!(e.kind, 6 | 11);
        let low = blow.is_some_and(|m| m.height == moves::Height::Low);
        let fallback = vec3(
            self.bodies[target].x + victim.facing as f32 * 0.18,
            self.bodies[target].y + if low { 0.3 } else { 1.3 },
            0.22,
        );
        self.impact = match &self.model {
            Some(model) if blow.is_some() => {
                let limb = model.striker_world(1 - target, e.attacker_action);
                let body_x = self.bodies[target].x + victim.facing as f32 * 0.12;
                vec3(limb.x * 0.6 + body_x * 0.4, limb.y, limb.z.max(0.1))
            }
            _ => fallback,
        };
        self.impact_kind = e.kind;
        self.flash = 1.0;
        if preview {
            return;
        }
        let dir = -victim.facing.signum() as f32;
        let facing = victim.facing.signum() as f32;
        self.push_bodies(&e);
        match e.kind {
            2 => {
                // The guard holds: a barrier flashes in front of the defender
                // at the height of the blow, sparks glance off it.
                let at = vec3(self.bodies[target].x + facing * 0.34, self.impact.y.clamp(0.35, 1.6), 0.18);
                self.effects.shield(at, facing, if heavy { 0.55 } else { 0.42 }, Color::hex(0x8FEFFF));
                self.effects.sparks(at, dir, if heavy { 16 } else { 10 }, 4.5, Color::hex(0xCFF6FF));
                self.guard_flash[target] = 1.0;
                self.shake = self.shake.max(if heavy { 0.3 } else { 0.18 });
            }
            9 => {
                let mid = (self.bodies[0].x + self.bodies[1].x) * 0.5;
                self.effects.sparks(vec3(mid, 1.25, 0.2), 0.0, 18, 5.0, Color::hex(0xFFFFFF));
                self.effects.ring(vec3(mid, 0.03, 0.0), 1.2, Color::hex(0xDDF8FF));
                self.hit_flash = [0.7; 2];
                self.shake = self.shake.max(0.3);
            }
            3 => {
                self.effects.sparks(self.impact, dir, 16, 5.5, Color::hex(0x7EEDFF));
                self.hit_flash[1 - target] = 0.8;
                self.shake = self.shake.max(0.35);
            }
            _ => {
                let count = if heavy { 26 } else { 15 };
                self.effects.sparks(self.impact, dir, count, if heavy { 7.5 } else { 5.5 }, Color::hex(0xFFD58B));
                self.effects.sparks(self.impact, dir, count / 3, 3.0, Color::hex(0xFFFFFF));
                self.hit_flash[target] = 1.0;
                self.shake = self.shake.max(if heavy { 0.75 } else { 0.32 });
                if heavy {
                    self.punch = 1.0;
                }
            }
        }
    }

    /// The physical layer: the struck part of the body gives way — the head
    /// snaps back from a jab and sideways from a hook, a kick folds the
    /// body, a low takes the legs, a blocked blow knocks the forearms back
    /// and a parried one bounces the attacker's arms.
    fn push_bodies(&mut self, e: &Event) {
        let Some(model) = &mut self.model else { return };
        let (target, other) = (e.target, 1 - e.target);
        // From the attacker towards the struck body.
        let away = if self.bodies[target].x >= self.bodies[other].x { 1.0 } else { -1.0 };
        let m = moves::attack_for(self.fighters[other].style, e.attacker_action);
        let heavy = m.is_some_and(|m| m.heavy());
        match e.kind {
            2 => model.push(target, Part::Guard, vec3(away, 0.15, 0.0), if heavy { 2.4 } else { 1.5 }),
            3 => model.push(other, Part::Guard, vec3(-away, 0.5, 0.0), 2.8),
            4 => model.push(target, Part::Chest, vec3(away, 0.3, 0.0), 2.8),
            8 => model.push(other, Part::Chest, vec3(-away, 0.2, 0.0), 2.6),
            9 => {
                model.push(target, Part::Chest, vec3(away, 0.2, 0.0), 1.8);
                model.push(other, Part::Chest, vec3(-away, 0.2, 0.0), 1.8);
            }
            10 => model.push(target, Part::Body, vec3(0.0, -1.0, 0.0), 2.2),
            5 => {}
            _ => {
                let (part, dir) = match e.attacker_action {
                    17 => (Part::Head, vec3(away * 0.6, 0.05, 0.8)),
                    10 => (Part::Head, vec3(away * 0.4, 1.0, 0.0)),
                    2 => (Part::Head, vec3(away, -0.3, 0.0)),
                    12 => (Part::Head, vec3(away, 0.2, 0.3)),
                    8 | 14 | 18 | 19 => (Part::Gut, vec3(away, 0.0, 0.0)),
                    9 => (Part::Legs, vec3(-away, 0.5, 0.0)),
                    16 => (Part::Legs, vec3(away * 0.4, 0.3, 0.0)),
                    _ => (Part::Head, vec3(away, 0.1, 0.0)),
                };
                let speed = if heavy { 3.4 } else { 2.0 } * if matches!(e.kind, 6 | 11) { 1.3 } else { 1.0 };
                model.push(target, part, dir, speed);
            }
        }
    }

    /// Training overlay: each fighter's boxes as the simulation sees them
    /// (combat/src/boxes.rs) at the shown position — body green, stretched
    /// limb yellow, active blow red, pushbox grey.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn draw_boxes(&self, g: &mut Graphics, camera: &Camera, c: Vec2) {
        use arena_combat::boxes;
        for side in 0..2 {
            let mut f = self.fighters[side].clone();
            f.x = (self.bodies[side].x * 1000.0).round() as i32;
            f.y = (self.bodies[side].y * 1000.0).round() as i32;
            f.frame = self.bodies[side].frame.floor().max(0.0) as u32;
            let m = f.attack();
            let [body, limb] = boxes::hurtboxes(&f, m.as_ref());
            let hit = m.as_ref().and_then(|m| boxes::hitbox(&f, m));
            let shapes = [
                (Some(boxes::pushbox(&f)), Color::hex(0x9AA4AE), 1.0, 0.012),
                (body, Color::hex(0x5BF08A), 2.0, 0.0),
                (limb, Color::hex(0xFFD24A), 2.0, 0.0),
                (hit, Color::hex(0xFF4A4A), 3.0, -0.012),
            ];
            for (rect, color, width, inset) in shapes {
                let Some(r) = rect else { continue };
                let (x0, x1) = (r.x0 as f32 / 1000.0 + inset, r.x1 as f32 / 1000.0 - inset);
                let (y0, y1) = (r.y0 as f32 / 1000.0 + inset, r.y1 as f32 / 1000.0 - inset);
                let corners = [vec3(x0, y0, 0.0), vec3(x1, y0, 0.0), vec3(x1, y1, 0.0), vec3(x0, y1, 0.0)];
                for k in 0..4 {
                    let (a, b) = (camera.project(corners[k], c), camera.project(corners[(k + 1) % 4], c));
                    g.line(a, b, width, color.with_alpha(0.9));
                }
            }
        }
    }

    fn preview(&self) -> bool {
        self.debug_camera.is_some_and(|c| c[7] > 0.5)
    }

    /// Everything visual, at the display's refresh rate.
    fn animate(&mut self, dt: f32, render_tick: f64) {
        // Knockout: the final blow plays out in slow motion, then eases back.
        const SLOWMO: f32 = 1.1;
        let real_dt = dt;
        let speed = if self.slowmo > 0.0 {
            let k = 1.0 - self.slowmo / SLOWMO;
            0.22 + 0.78 * k * k
        } else {
            1.0
        };
        let dt = dt * speed;
        self.slowmo = (self.slowmo - real_dt).max(0.0);
        // The simulation does not slow down: the view falls behind it,
        // never runs backwards, and catches up at 1.6 times the speed.
        let render_tick = if !render_tick.is_finite() {
            render_tick
        } else if !self.view.is_finite() || (render_tick - self.view).abs() > 90.0 {
            render_tick
        } else {
            let rate = if speed < 1.0 { speed as f64 } else { 1.6 };
            (self.view + real_dt as f64 * 60.0 * rate).min(render_tick).max(self.view)
        };
        self.view = render_tick;
        let Some(sample) = self.timeline.sample(render_tick) else {
            return;
        };
        let current = self.timeline.get(sample.index).cloned().unwrap_or_else(|| self.state.clone());
        self.fighters = sample.fighters;
        self.bodies = sample.bodies;
        let preview = self.preview();
        for side in 0..2 {
            if self.fighters[side].hp == 0 && !self.knocked[side] {
                self.knocked[side] = true;
                if !preview {
                    self.slowmo = SLOWMO;
                    self.shake = 1.0;
                    self.punch = 1.0;
                }
            }
        }
        if current.phase != self.phase {
            self.phase = current.phase;
            self.phase_time = 0.0;
        } else {
            self.phase_time += dt;
        }
        while let Some(e) = self.pending.front() {
            let due = !render_tick.is_finite() || e.tick as f64 <= render_tick + 0.5;
            if !due {
                break;
            }
            let e = self.pending.pop_front().unwrap();
            self.fire(e);
        }
        // Take-offs and special moves leave marks on the floor.
        for side in 0..2 {
            let f = &self.fighters[side];
            let (action, frame) = (f.action, self.bodies[side].frame);
            let (last_action, last_frame) = self.last_action[side];
            if !preview {
                let x = self.bodies[side].x;
                if action == 3 && last_action != 3 {
                    self.effects.dust(vec3(x, 0.0, 0.0), 6, 0.6);
                }
                if let Some(m) = f.attack() {
                    let hit = m.startup as f32;
                    let crossed = action == last_action && last_frame < hit && frame >= hit
                        || action != last_action && frame >= hit && frame < hit + 2.0;
                    if crossed && action == 19 {
                        self.effects.ring(vec3(x, 0.03, 0.0), 2.1, Color::hex(0xFFD293));
                        self.effects.dust(vec3(x + f.facing as f32 * 0.4, 0.0, 0.0), 16, 1.4);
                        self.shake = self.shake.max(0.6);
                    }
                    if crossed && action == 14 {
                        let at = vec3(x + f.facing as f32 * 0.7, 1.2, 0.1);
                        self.effects.ring(vec3(x, 0.03, 0.0), 1.6, Color::hex(0x8BFFE9));
                        self.effects.sparks(at, f.facing as f32, 18, 6.5, Color::hex(0x9DFFF0));
                    }
                }
            }
            self.last_action[side] = (action, frame);
        }
        if let Some(model) = &mut self.model {
            let mut visual = self.fighters.clone();
            if current.phase >= 2 {
                for f in &mut visual {
                    if f.hp > 0 {
                        f.action = 0;
                        f.guard = false;
                        f.crouch = false;
                        f.previous = 0;
                        f.stun = 0;
                        f.down = 0;
                    }
                }
            }
            let view = View {
                dt,
                time: self.time,
                phase: current.phase,
                winner: current.winner,
                phase_time: if preview { 10.0 } else { self.phase_time },
                freeze: current.freeze,
                victim: current.event_target.min(1),
                reaction: self.reaction,
                preview,
                variant: current.round,
            };
            model.update(&visual, &self.bodies, &view);
            for (at, strength) in model.take_impacts() {
                if !preview {
                    self.effects.dust(at, (8.0 + strength * 10.0) as usize, 0.7 + strength * 0.5);
                    self.shake = self.shake.max(0.25 * strength);
                }
            }
            for side in 0..2 {
                let f = &visual[side];
                let frame = self.bodies[side].frame;
                let active = f.attack().is_some_and(|m| {
                    frame + 3.0 >= m.startup as f32 && frame < (m.startup + m.active + 3) as f32
                });
                if active && !preview {
                    self.trails[side].push((model.striker_world(side, f.action), self.time));
                }
                let now = self.time;
                self.trails[side].retain(|(_, t)| now - *t < 0.09);
            }
        }
        for (at, kind) in self.props.take_breaks() {
            if preview {
                continue;
            }
            match kind {
                1 => self.effects.sparks(at, 0.0, 22, 3.5, Color::hex(0xCFF6FF)),
                7 => self.effects.dust(at + vec3(0.0, 0.05, 0.0), 14, 1.6),
                _ => self.effects.dust(vec3(at.x, (at.y * 0.5).min(1.2), at.z.min(-0.3)), 14, 1.2),
            }
        }
        self.effects.update(dt);
        let fade = (-dt * 7.0).exp();
        for f in &mut self.hit_flash {
            *f *= fade;
        }
        let guard_fade = (-dt * 5.0).exp();
        for f in &mut self.guard_flash {
            *f *= guard_fade;
        }
        self.flash = (self.flash - dt * 3.5).max(0.0);
        self.shake *= (-dt * 8.0).exp();
        self.punch *= (-dt * 6.0).exp();
    }

    fn camera(&mut self, canvas: Vec2, dt: f32) -> Camera {
        let aspect = canvas.x / canvas.y;
        let [top, bottom, left, right] = self.layout;
        let band = (bottom - top).clamp(0.3, 1.0);
        let across = (right - left).clamp(0.3, 1.0);
        let tan = (FOV * 0.5).tan();
        let xs = [self.bodies[0].x, self.bodies[1].x];
        let margin = if aspect < 1.0 { 1.5 } else { 1.9 };
        let span = (xs[1] - xs[0]).abs() + margin;
        let by_width = span / (2.0 * tan * aspect * across);
        let by_height = 2.3 / (2.0 * tan * band);
        let mut wanted = by_width.max(by_height).max(4.2) * (1.0 - 0.05 * self.punch);
        let mut center = (xs[0] + xs[1]) * 0.5;
        // Match over: the camera eases in on the winner's dance.
        let winner = self.state.winner;
        let dancing = self.state.phase == 3 && winner >= 0 && self.phase_time > 2.2 && !self.preview();
        self.celebrate += (f32::from(u8::from(dancing)) - self.celebrate) * (1.0 - (-dt * 1.2).exp());
        let c = self.celebrate * self.celebrate * (3.0 - 2.0 * self.celebrate);
        if c > 0.0 {
            let x = self.bodies[winner.clamp(0, 1) as usize].x;
            center += (x - center) * c;
            // A dancer needs no room for jumps: frame the body, not 2.3 m.
            wanted += ((by_height * 0.8).max(3.2) - wanted) * c;
        }
        let k = 1.0 - (-dt * 7.0).exp();
        self.view_distance += (wanted - self.view_distance) * k;
        self.view_center += (center - self.view_center) * (1.0 - (-dt * 10.0).exp());
        let lift = (self.bodies[0].y.max(self.bodies[1].y) * 0.35).min(0.5);
        self.view_lift += (lift - self.view_lift) * (1.0 - (-dt * 5.0).exp());
        let d = self.view_distance;
        let shake = vec3(
            (self.time * 83.0).sin() * 0.045 * self.shake,
            (self.time * 67.0).cos() * 0.03 * self.shake,
            0.0,
        );
        let target = vec3(self.view_center, 0.92 + self.view_lift, 0.0);
        let orbit = vec3((self.time * 0.4).sin() * 0.7 * c, 0.0, 0.0);
        let mut camera = Camera {
            eye: target + vec3(0.0, d * 0.13, d) + shake + orbit,
            target: target + shake * 0.5,
            fov_y: FOV,
            near: 0.1,
            far: 100.0,
            // The floor line sits near the bottom of the free band; the rest
            // of the band shows the fighters and the room above them.
            shift: vec2(left + right - 1.0, 1.0 - 2.0 * (top + 0.9 * band) + 0.92 / (d * tan)),
        };
        if let Some(c) = self.debug_camera {
            camera.eye = vec3(c[0], c[1], c[2]);
            camera.target = vec3(c[3], c[4], c[5]);
            camera.fov_y = c[6];
            camera.shift = Vec2::ZERO;
        }
        camera
    }
}

fn body_mesh(color: Vec3) -> MeshData {
    let mut mesh = MeshData::default();
    for joint in 0..16 {
        let c = match joint {
            3 => vec3(0.85, 0.98, 1.0),
            1 | 4 | 7 | 10 | 13 => vec3(0.065, 0.085, 0.12),
            _ => color,
        };
        mesh.add_sphere(Vec3::ZERO, vec3(1.0, 1.0, 1.0), 12, 8, joint, c);
    }
    // The legacy sphere builder winds toward the interior; these solid armour
    // pieces need outward front faces under back-face culling.
    for triangle in mesh.indices.chunks_exact_mut(3) {
        triangle.swap(1, 2);
    }
    mesh
}
fn ellipsoid(pos: Vec3, scale: Vec3) -> Mat4 {
    Mat4::from_trs(pos, Quat::IDENTITY, scale)
}
fn limb(a: Vec3, b: Vec3, radius: f32) -> Mat4 {
    Mat4::from_trs(
        (a + b) * 0.5,
        Quat::from_rotation_arc(Vec3::Y, (b - a).normalize()),
        vec3(radius, (b - a).length() * 0.58, radius),
    )
}
/// Placeholder figure shown while the textured model streams in.
fn pose(f: &Fighter, time: f32) -> Vec<Mat4> {
    let bob = (time * 7.0).sin() * 0.025;
    let mut out = vec![Mat4::IDENTITY; 16];
    out[0] = ellipsoid(vec3(0.0, 1.15 + bob, 0.0), vec3(0.24, 0.34, 0.20));
    out[1] = ellipsoid(vec3(0.0, 0.84 + bob, 0.0), vec3(0.22, 0.18, 0.18));
    out[2] = ellipsoid(vec3(0.03, 1.64 + bob, 0.0), vec3(0.19, 0.23, 0.19));
    out[3] = ellipsoid(vec3(0.17, 1.68 + bob, 0.07), vec3(0.065, 0.07, 0.17));
    for side in 0..2 {
        let z = if side == 0 { 0.24 } else { -0.24 };
        let shoulder = vec3(0.0, 1.35 + bob, z);
        let elbow = vec3(0.12, 1.04 + bob, z * 1.3);
        let fist = if f.guard {
            vec3(0.28, 1.62 + bob, z * 0.6)
        } else {
            vec3(0.38, 1.39 + bob, z)
        };
        let j = 4 + side * 3;
        out[j] = limb(shoulder, elbow, 0.12);
        out[j + 1] = limb(elbow, fist, 0.115);
        out[j + 2] = ellipsoid(fist, vec3(0.155, 0.145, 0.155));
        let foot_x = if side == 0 { 0.34 } else { -0.30 };
        let hip = vec3(0.0, 0.83 + bob, z * 0.5);
        let knee = vec3(foot_x * 0.7 + 0.05, 0.46 + bob, z * 0.75);
        let foot = vec3(foot_x, 0.12, z * 0.9);
        let j = 10 + side * 3;
        out[j] = limb(hip, knee, 0.14);
        out[j + 1] = limb(knee, foot, 0.12);
        out[j + 2] = ellipsoid(foot + vec3(0.04, -0.015, 0.0), vec3(0.22, 0.105, 0.15));
    }
    out
}

impl FightScene {
    /// Starts fetching bodies the page asks for and shows each side's body
    /// once it is ready (the default fighter stands in meanwhile).
    fn update_avatars(&mut self) {
        let Some(model) = &mut self.model else { return };
        let mut which = [0usize; 2];
        let mut ready = 0;
        for side in 0..2 {
            let (path, pack) = self.wanted[side].clone();
            if path == DEFAULT_MODEL {
                ready |= 1 << side;
                continue;
            }
            let slot = self.avatars.entry(path.clone()).or_insert_with(|| AvatarSlot::Loading {
                model: fetch(&path),
                pack: fetch(&pack),
            });
            if let AvatarSlot::Loading { model: bytes, pack } = slot {
                let ready = bytes.borrow().is_some() && pack.borrow().is_some();
                if ready {
                    let body = bytes.borrow_mut().take().unwrap();
                    let clips = pack.borrow_mut().take().unwrap();
                    *slot = match body.and_then(|b| model.add_avatar(&b)) {
                        Ok(index) => {
                            if let Ok(clips) = clips {
                                if let Err(e) = model.set_captured(index, &clips) {
                                    eprintln!("Fight pack {path}: {e}");
                                }
                            }
                            AvatarSlot::Ready(index)
                        }
                        Err(e) => {
                            eprintln!("Fighter {path}: {e}");
                            AvatarSlot::Failed
                        }
                    };
                }
            }
            match slot {
                AvatarSlot::Ready(index) => {
                    which[side] = *index;
                    ready |= 1 << side;
                }
                // A body that failed to load falls back to the default.
                AvatarSlot::Failed => ready |= 1 << side,
                AvatarSlot::Loading { .. } => {}
            }
        }
        model.show(which);
        if ready != self.avatars_ready {
            self.avatars_ready = ready;
            #[cfg(target_arch = "wasm32")]
            unsafe {
                fight_avatars(ready);
            }
        }
    }
}

impl Scene for FightScene {
    fn update(&mut self, _ctx: &FrameCtx) -> Transition {
        #[cfg(target_arch = "wasm32")]
        {
            let mut bytes = [0u8; 1024];
            let n = unsafe { fight_fighters(bytes.as_mut_ptr(), bytes.len()) };
            if n > 0 && n <= bytes.len() {
                if let Ok(text) = std::str::from_utf8(&bytes[..n]) {
                    let lines: Vec<&str> = text.lines().collect();
                    if lines.len() >= 4 {
                        self.wanted = [
                            (lines[0].to_string(), lines[1].to_string()),
                            (lines[2].to_string(), lines[3].to_string()),
                        ];
                    }
                }
            }
        }
        self.update_avatars();
        let room = self.room.borrow_mut().take();
        if let Some(result) = room {
            #[cfg(target_arch = "wasm32")]
            let cap = unsafe { fight_texture_cap() }.max(0) as u32;
            #[cfg(not(target_arch = "wasm32"))]
            let cap = 0;
            let status = match result.and_then(|bytes| self.props.set_room(&bytes, cap)) {
                Ok(n) => {
                    eprintln!("Room: {n} breakable pieces");
                    1
                }
                // No baked room: the box room stays.
                Err(e) => {
                    eprintln!("Room: {e}");
                    -1
                }
            };
            #[cfg(target_arch = "wasm32")]
            unsafe {
                fight_room_status(status);
            }
            let _ = status;
        }
        let loaded = self.loading.borrow_mut().take();
        if let Some(result) = loaded {
            match result.and_then(|bytes| FighterModel::load(&bytes)) {
                Ok(model) => {
                    self.model = Some(model);
                    #[cfg(target_arch = "wasm32")]
                    unsafe {
                        fight_model_status(1);
                    }
                }
                Err(message) => {
                    eprintln!("Fighter model: {message}");
                    #[cfg(target_arch = "wasm32")]
                    unsafe {
                        fight_model_status(-1);
                    }
                }
            }
        }
        if let Some(model) = &mut self.model {
            let ready = self.pack.borrow_mut().take();
            match ready {
                Some(Ok(bytes)) => match model.set_captured(0, &bytes) {
                    Ok(n) => eprintln!("Fight pack: {n} captured clips"),
                    Err(e) => eprintln!("Fight pack: {e}"),
                },
                // No pack: authored animation only.
                Some(Err(_)) | None => {}
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut bytes = [0u8; 16384];
            let n = unsafe { fight_read(bytes.as_mut_ptr(), bytes.len()) };
            if n > 0 && n <= bytes.len() {
                if let Ok(json) = std::str::from_utf8(&bytes[..n]) {
                    if let Ok(state) = Match::deserialize_json(json) {
                        self.accept(state);
                    }
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use arena_combat::*;
            use miniquad::KeyCode::*;
            let mut input = 0;
            for (key, bit) in [
                (A, LEFT),
                (D, RIGHT),
                (S, BLOCK),
                (J, LIGHT),
                (K, HEAVY),
                (L, THROW),
                (U, KICK),
                (C, CROUCH),
                (W, JUMP),
                (I, SPECIAL),
                (Q, SMASH),
                (Space, DASH),
            ] {
                if _ctx.input.is_down(key) {
                    input |= bit;
                }
            }
            let mut next = if _ctx.input.just_pressed(R) {
                Match::default()
            } else {
                self.state.clone()
            };
            let bot = next.bot_input(1);
            next.step([input, bot]);
            self.accept(next);
        }
        let xs = [self.bodies[0].x, self.bodies[1].x];
        let latest = self.state.clone();
        self.props.update(&latest, xs);
        Transition::None
    }

    fn draw(&mut self, g: &mut Graphics, alpha: f32) {
        let now = miniquad::date::now();
        let dt = ((now - self.last_draw) as f32).clamp(0.0, 0.1);
        self.last_draw = now;
        self.time += dt;
        #[cfg(target_arch = "wasm32")]
        let render_tick = {
            let mut cam = [-1.0f32; 10];
            let debug = unsafe { fight_debug_camera(cam.as_mut_ptr()) } == 1;
            self.debug_camera = debug.then(|| [cam[0], cam[1], cam[2], cam[3], cam[4], cam[5], cam[6], cam[7]]);
            if let Some(model) = &mut self.model {
                model.review = (debug && cam[8] >= 0.0).then(|| (cam[8] as usize, cam[9].max(0.0)));
            }
            let mut layout = [0.0f32, 0.0, 0.0, 1.0];
            if unsafe { fight_layout(layout.as_mut_ptr()) } == 1
                && layout[1] > layout[0]
                && layout[3] > layout[2]
                && layout.iter().all(|v| v.is_finite())
            {
                self.layout = [
                    layout[0].clamp(0.0, 0.9),
                    layout[1].clamp(0.1, 1.0),
                    layout[2].clamp(0.0, 0.7),
                    layout[3].clamp(0.3, 1.0),
                ];
            }
            let clock = unsafe { fight_clock() };
            if self.preview() || clock < 0.0 {
                f64::NAN
            } else {
                clock
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        let render_tick = self.state.tick as f64 - 1.0 + alpha as f64;
        let _ = alpha;
        self.animate(dt, render_tick);

        if self.meshes.is_empty() {
            self.meshes
                .push(g.upload_skinned(&body_mesh(vec3(0.08, 0.82, 0.73)), &[], &[]));
            self.meshes
                .push(g.upload_skinned(&body_mesh(vec3(0.95, 0.25, 0.20)), &[], &[]));
        }
        let c = g.canvas();
        let camera = self.camera(c, dt);
        g.rect(rect(0.0, 0.0, c.x, c.y), Color::hex(0x080E18));
        // Open-front room: the camera sees the playable floor and three walls.
        // The finish, beams and furniture are rendered by ArenaProps.
        let floor = camera.project(vec3(0.0, -0.05, -2.0), c).y;
        g.rect(rect(0.0, floor, c.x, (c.y - floor).max(0.0)), Color::hex(0x141C23));
        let lighting = if self.props.has_room() {
            // Matches the baked apartment: warm lamps overhead, a cool
            // bounce from the open side, shadows cast towards the viewer.
            Lighting {
                key_dir: vec3(0.25, 0.9, 0.45),
                key_color: Color::hex(0xFFDDB3),
                specular: 0.14,
                fill_dir: vec3(-0.2, 0.3, 1.0),
                fill_color: Color::hex(0x4A4F63),
                rim_color: Color::hex(0xFFD9A8),
                rim_strength: 0.18,
                shadow_dir: vec3(0.22, 1.0, -0.3),
            }
        } else {
            Lighting {
                rim_color: Color::hex(0xACF4FF),
                rim_strength: 0.18,
                ..Lighting::default()
            }
        };
        self.scenery.update(g);
        self.props.hide_city = self.scenery.has_city();
        self.scenery.draw_backdrop(g, &camera);
        self.props.draw(g, &self.state, &camera, &lighting);
        self.scenery.draw_screens(g, &camera, &self.props);
        if let Some(model) = &mut self.model {
            model.upload(g);
            for i in 0..2 {
                let team = Color::hex(if i == 0 { 0x57F0CE } else { 0xFF7965 });
                let flash = self.hit_flash[i];
                let white = Color::hex(0xFFFFFF);
                // Window light in the garden gradually replaces the warm pendants.
                let cool = ((-self.bodies[i].x - 7.8) / 1.5).clamp(0.0, 1.0);
                let east = Color::hex(0xC7DDFF);
                let key_color = Color {
                    r: lighting.key_color.r + (east.r - lighting.key_color.r) * cool,
                    g: lighting.key_color.g + (east.g - lighting.key_color.g) * cool,
                    b: lighting.key_color.b + (east.b - lighting.key_color.b) * cool,
                    a: 1.0,
                };
                // A raised guard reads at a glance: a steady cyan rim, flaring
                // when a blow lands on it.
                let guarding = self.fighters[i].guard && self.fighters[i].hp > 0;
                let shield = (if guarding { 0.55 } else { 0.0 } + self.guard_flash[i]).min(1.0) * (1.0 - flash);
                let cyan = Color::hex(0x8FEFFF);
                let rim = Color {
                    r: team.r + (white.r - team.r) * flash,
                    g: team.g + (white.g - team.g) * flash,
                    b: team.b + (white.b - team.b) * flash,
                    a: 1.0,
                };
                let team_light = Lighting {
                    key_color,
                    rim_color: Color {
                        r: rim.r + (cyan.r - rim.r) * shield,
                        g: rim.g + (cyan.g - rim.g) * shield,
                        b: rim.b + (cyan.b - rim.b) * shield,
                        a: 1.0,
                    },
                    rim_strength: 0.38 + flash * 1.5 + shield * 0.9,
                    ..lighting
                };
                g.draw_skinned(
                    model.avatar(i).character.mesh.as_ref().unwrap(),
                    &model.skin[i],
                    model.placement[i],
                    &camera,
                    &team_light,
                    0.35,
                );
                let root = model.root(i);
                let p = camera.project(vec3(root.x, 0.015, 0.1), c);
                let fade = (1.0 - root.y * 0.6).clamp(0.3, 1.0);
                let size = (c.y / 18.0).clamp(30.0, 60.0);
                g.blob(
                    rect(p.x - size * fade, p.y - size * 0.27 * fade, size * 2.0 * fade, size * 0.54 * fade),
                    team.with_alpha(0.75 * fade),
                );
            }
        } else {
            for i in 0..2 {
                let f = &self.fighters[i];
                let model = Mat4::from_trs(
                    vec3(self.bodies[i].x, self.bodies[i].y, 0.0),
                    Quat::from_axis_angle(
                        Vec3::Y,
                        if i == 0 {
                            -0.18
                        } else {
                            std::f32::consts::PI + 0.18
                        },
                    ),
                    vec3(1.0, 1.0, 1.0),
                );
                g.draw_skinned(&self.meshes[i], &pose(f, self.time), model, &camera, &lighting, 0.35);
            }
        }
        // Motion smears behind the striking limb.
        for side in 0..2 {
            let points = &self.trails[side];
            let color = Color::hex(if side == 0 { 0xB6FFEE } else { 0xFFD3B8 });
            for (i, pair) in points.windows(2).enumerate() {
                let age = (self.time - pair[1].1) / 0.09;
                let k = (1.0 - age).clamp(0.0, 1.0) * (i + 1) as f32 / points.len() as f32;
                g.line(
                    camera.project(pair[0].0, c),
                    camera.project(pair[1].0, c),
                    2.0 + 7.0 * k,
                    color.with_alpha(0.55 * k),
                );
            }
        }
        self.effects.draw(g, &camera, c);
        #[cfg(target_arch = "wasm32")]
        if unsafe { fight_flags() } & 1 != 0 {
            self.draw_boxes(g, &camera, c);
        }
        if self.flash > 0.0 {
            let p = camera.project(self.impact, c);
            let col = match self.impact_kind {
                2 | 3 => Color::hex(0x9EEFFF),
                _ => Color::hex(0xFFE2A8),
            };
            let r = 20.0 + (1.0 - self.flash) * 26.0;
            g.blob(
                rect(p.x - r, p.y - r, r * 2.0, r * 2.0),
                col.with_alpha(self.flash * self.flash * 0.7),
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            g.text_centered(
                "PULSE / ARENA",
                vec2(c.x * 0.5, 25.0),
                2.0,
                Color::hex(0xFFFFFF),
            );
            for i in 0..2 {
                let x = if i == 0 { 20.0 } else { c.x - 170.0 };
                g.rect(rect(x, 48.0, 150.0, 8.0), Color::hex(0x25374A));
                g.rect(
                    rect(x, 48.0, self.state.fighters[i].hp as f32 * 1.5, 8.0),
                    Color::hex(if i == 0 { 0x32DFBC } else { 0xFF705A }),
                );
            }
            g.text_centered(
                &format!(
                    "{} : {}     {}",
                    self.state.score[0],
                    self.state.score[1],
                    self.state.remaining / 60
                ),
                vec2(c.x * 0.5, 70.0),
                1.5,
                Color::hex(0xFFFFFF),
            );
            g.text_centered(
                "A/D MOVE  S GUARD  J JAB  K HEAVY  L GRAB  SPACE DASH  R RESET",
                vec2(c.x * 0.5, c.y - 25.0),
                1.0,
                Color::hex(0xFFFFFF),
            );
            let label = match self.state.phase {
                0 => "GET READY",
                2 => "ROUND OVER",
                3 => {
                    if self.state.winner == 0 {
                        "YOU WIN / R TO REPLAY"
                    } else {
                        "DEFEAT / R TO REPLAY"
                    }
                }
                _ => "",
            };
            g.text_centered(label, vec2(c.x * 0.5, c.y * 0.27), 2.0, Color::hex(0xFFFFFF));
        }
    }
}
