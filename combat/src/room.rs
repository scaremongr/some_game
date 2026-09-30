//! Shared room layout and durable objects. IDs are stable across snapshots.
use nanoserde::{DeJson, SerJson};
pub const OBJECTS: usize = 20;
// kind: 0 back panel, 1 glass, 2 table, 3 stool, 4 cabinet,
// 5 light, 6 planter, 7 floor tile, 8 side partition, 9 overhead beam.
#[derive(Clone, Copy)]
pub struct ObjectDef {
    pub kind: u32,
    pub x: i32,
    pub z: i32,
    pub hp: i32,
}
pub const LAYOUT: [ObjectDef; OBJECTS] = [
    ObjectDef {
        kind: 0,
        x: -3600,
        z: -1800,
        hp: 48,
    },
    ObjectDef {
        kind: 0,
        x: -1800,
        z: -1800,
        hp: 48,
    },
    ObjectDef {
        kind: 0,
        x: 0,
        z: -1800,
        hp: 48,
    },
    ObjectDef {
        kind: 0,
        x: 1800,
        z: -1800,
        hp: 48,
    },
    ObjectDef {
        kind: 0,
        x: 3600,
        z: -1800,
        hp: 48,
    },
    ObjectDef {
        kind: 1,
        x: -2600,
        z: -1680,
        hp: 20,
    },
    ObjectDef {
        kind: 1,
        x: 2600,
        z: -1680,
        hp: 20,
    },
    ObjectDef {
        kind: 2,
        x: -1450,
        z: -700,
        hp: 32,
    },
    ObjectDef {
        kind: 2,
        x: 1450,
        z: -700,
        hp: 32,
    },
    ObjectDef {
        kind: 3,
        x: -600,
        z: -650,
        hp: 18,
    },
    ObjectDef {
        kind: 3,
        x: 650,
        z: -650,
        hp: 18,
    },
    ObjectDef {
        kind: 4,
        x: -3750,
        z: -950,
        hp: 38,
    },
    ObjectDef {
        kind: 4,
        x: 3750,
        z: -950,
        hp: 38,
    },
    ObjectDef {
        kind: 5,
        x: -1600,
        z: -450,
        hp: 20,
    },
    ObjectDef {
        kind: 5,
        x: 1600,
        z: -450,
        hp: 20,
    },
    ObjectDef {
        kind: 6,
        x: 0,
        z: -1250,
        hp: 22,
    },
    ObjectDef {
        kind: 7,
        x: -2100,
        z: 0,
        hp: 45,
    },
    ObjectDef {
        kind: 7,
        x: 2100,
        z: 0,
        hp: 45,
    },
    ObjectDef {
        kind: 8,
        x: -4650,
        z: -300,
        hp: 60,
    },
    ObjectDef {
        kind: 8,
        x: 4650,
        z: -300,
        hp: 60,
    },
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
