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
impl Move {
    /// Heavy blows get the longer hitstop, the bigger recoil and effects.
    pub fn heavy(&self) -> bool {
        self.damage >= 13
    }
}
/// Ticks after the grab during which the victim can break a throw (THROW).
pub const TECH: u32 = 10;
pub fn attack(action: u32) -> Option<Move> {
    use Height::*;
    // startup, active, recovery end, reach, damage, hitstun, blockstun,
    // stamina cost, horizontal impulse, launch velocity, height, knockdown.
    // Advantage on block (measured by the tests): jab -1, low kick -3,
    // cross/kick -4 keep pressure; heavy, sweep, uppercut and roundhouse are
    // punishable; the special buys safety with meter.
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
        1 => (7, 3, 20, 1180, 7, 22, 11, 50, 18, 0, High, false),
        2 => (21, 3, 47, 1550, 17, 30, 14, 220, 165, 42, Overhead, true),
        // The throw holds its victim for HOLD ticks after the grab (lib.rs).
        4 => (13, 2, 56, 1150, 14, 35, 0, 180, 0, 0, Grab, true),
        8 => (11, 4, 30, 1640, 9, 26, 14, 90, 35, 0, Mid, false),
        9 => (14, 3, 42, 1540, 11, 32, 11, 140, 55, 0, Low, true),
        // Rising uppercut from a crouch: anti-air, beats high attacks while
        // it rises (lib.rs), launches; very unsafe on block.
        10 => (12, 4, 46, 1250, 13, 42, 12, 180, 32, 110, Mid, true),
        11 => (6, 3, 24, 1300, 7, 24, 13, 50, 24, 0, Mid, false),
        12 => (12, 4, 38, 1780, 14, 30, 14, 150, 145, 45, Mid, true),
        13 => (7, 6, 28, 1580, 10, 26, 15, 100, 55, 0, Overhead, false),
        14 => (18, 5, 46, 2150, 20, 34, 26, 160, 190, 55, Mid, true),
        // Quick low kick from a crouch: opens a standing guard, no knockdown.
        16 => (8, 3, 24, 1450, 5, 20, 12, 60, 20, 0, Low, false),
        // Third punch of J-J-J: a lead hook, a little push.
        17 => (8, 3, 28, 1200, 8, 26, 14, 60, 70, 0, Mid, false),
        // Second kick of U-U: a side kick that drives the opponent back
        // (towards a wall or the next room); safe by distance on block.
        18 => (12, 4, 34, 1720, 9, 28, 16, 100, 150, 0, Mid, false),
        19 => (18, 3, 44, 950, 12, 24, 10, 140, 45, 0, Mid, false),
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
/// Follow-ups allowed once the attack has hit.
pub fn cancel(action: u32, next: u32) -> bool {
    string(action, next)
        || matches!(
            (action, next),
            (1, 2)
                | (1, 8)
                | (11, 12)
                | (11, 10)
                | (11, 14)
                | (8, 10)
                | (8, 14)
                | (17, 10)
                | (18, 12)
                | (16, 1)
                | (16, 9)
                | (16, 10)
        )
}
/// Follow-ups allowed when the attack was blocked: light strings keep the
/// pressure; the defender can interrupt the gap before a slower ender.
pub fn block_cancel(action: u32, next: u32) -> bool {
    string(action, next) || matches!((action, next), (1, 8) | (11, 12) | (16, 1))
}
/// Strings: pressing the same button again continues with a different move
/// even on a whiff (J-J-J jab, cross, hook; U-U front kick, side kick).
/// On a whiff the next move waits for the active frames to end.
pub fn string(action: u32, next: u32) -> bool {
    matches!((action, next), (1, 11) | (11, 17) | (8, 18))
}
