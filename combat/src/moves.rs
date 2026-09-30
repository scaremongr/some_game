//! Frame data shared by simulation and animation. All times are 60 Hz ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Height {
    High,
    Mid,
    Low,
    Overhead,
    Grab,
}
#[derive(Clone, Copy, Debug)]
pub struct Move {
    pub startup: u32,
    pub active: u32,
    pub total: u32,
    pub reach: i32,
    pub damage: i32,
    pub stun: u32,
    pub blockstun: u32,
    pub cost: i32,
    pub push: i32,
    pub launch: i32,
    pub height: Height,
    pub knockdown: bool,
}
pub fn attack(action: u32) -> Option<Move> {
    use Height::*;
    // startup, active, recovery end, reach, damage, hitstun, blockstun,
    // stamina cost, horizontal impulse, launch velocity, height, knockdown
    let (
        startup,
        active,
        total,
        reach,
        damage,
        stun,
        blockstun,
        cost,
        push,
        launch,
        height,
        knockdown,
    ) = match action {
        1 => (7, 3, 24, 1180, 8, 23, 10, 70, 18, 0, High, false),
        2 => (21, 3, 49, 1550, 21, 30, 12, 220, 165, 42, Overhead, true),
        // The throw holds its victim for HOLD ticks after the grab (lib.rs).
        4 => (13, 2, 56, 1150, 17, 35, 0, 180, 0, 0, Grab, true),
        8 => (11, 4, 32, 1640, 11, 26, 13, 100, 35, 0, Mid, false),
        9 => (16, 3, 43, 1540, 13, 32, 11, 140, 55, 0, Low, true),
        10 => (13, 4, 45, 1250, 15, 42, 12, 180, 32, 110, Mid, true),
        11 => (6, 3, 25, 1300, 9, 25, 11, 70, 24, 0, Mid, false),
        12 => (12, 4, 40, 1780, 17, 30, 12, 150, 145, 45, Mid, true),
        13 => (7, 6, 28, 1580, 12, 26, 15, 100, 55, 0, Overhead, false),
        14 => (18, 5, 53, 2150, 25, 34, 14, 160, 190, 55, Mid, true),
        19 => (18, 3, 44, 950, 14, 24, 10, 140, 45, 0, Mid, false),
        _ => return None,
    };
    Some(Move {
        startup,
        active,
        total,
        reach,
        damage,
        stun,
        blockstun,
        cost,
        push,
        launch,
        height,
        knockdown,
    })
}
pub fn cancel(action: u32, next: u32) -> bool {
    matches!(
        (action, next),
        (1, 11) | (1, 2) | (1, 8) | (11, 12) | (11, 10) | (8, 10) | (8, 14) | (11, 14)
    )
}
