//! Shared room layout and durable objects. IDs are stable across snapshots.
//! IDs 0..4 describe the permanent exterior wall and are never damaged.
use nanoserde::{DeJson, SerJson};
pub const OBJECTS: usize = 20;
/// One continuous fighting lane through five open rooms (millimetres).
pub const ARENA_LIMIT: i32 = 11500;
pub const ROUND_CENTERS: [i32; 5] = [0, -5500, 5500, -10000, 10000];
// kind: 0 fixed back panel, 1 glass, 2 table, 3 stool, 4 cabinet,
// 5 light, 6 planter, 7 floor tile, 8 side partition, 9 overhead beam.
#[derive(Clone, Copy)]
pub struct ObjectDef {
    pub kind: u32,
    pub x: i32,
    pub z: i32,
    pub hp: i32,
}
pub const LAYOUT: [ObjectDef; OBJECTS] = [
    ObjectDef { kind: 0, x: -10000, z: -3420, hp: 48 },
    ObjectDef { kind: 0, x: -5500, z: -3420, hp: 48 },
    ObjectDef { kind: 0, x: 0, z: -3420, hp: 48 },
    ObjectDef { kind: 0, x: 5500, z: -3420, hp: 48 },
    ObjectDef { kind: 0, x: 10000, z: -3420, hp: 48 },
    ObjectDef { kind: 1, x: -1570, z: -3340, hp: 20 },
    ObjectDef { kind: 1, x: 1570, z: -3340, hp: 20 },
    ObjectDef { kind: 2, x: -500, z: -950, hp: 32 },
    ObjectDef { kind: 2, x: 2400, z: -900, hp: 32 },
    ObjectDef { kind: 3, x: -5850, z: -1180, hp: 18 },
    ObjectDef { kind: 3, x: 6600, z: -1300, hp: 18 },
    ObjectDef { kind: 4, x: -3600, z: -2800, hp: 38 },
    ObjectDef { kind: 2, x: 5100, z: -1350, hp: 38 },
    ObjectDef { kind: 5, x: -1800, z: -2780, hp: 20 },
    ObjectDef { kind: 5, x: 1800, z: -2780, hp: 20 },
    ObjectDef { kind: 6, x: -2250, z: -1200, hp: 22 },
    ObjectDef { kind: 2, x: -10000, z: -1400, hp: 45 },
    ObjectDef { kind: 2, x: 8720, z: -1400, hp: 45 },
    ObjectDef { kind: 4, x: -8600, z: -2800, hp: 60 },
    ObjectDef { kind: 4, x: 11250, z: -2600, hp: 60 },
];
#[derive(Clone, Debug, SerJson, DeJson)]
pub struct ObjectState {
    pub hp: i32,
    pub broken_tick: u32,
    pub impulse: i32,
}
pub fn fresh() -> Vec<ObjectState> {
    LAYOUT
        .iter()
        .map(|o| ObjectState {
            hp: o.hp,
            broken_tick: 0,
            impulse: 0,
        })
        .collect()
}
