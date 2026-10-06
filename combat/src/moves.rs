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
        // Jump attacks by button: J a quick knee, U the flying kick (13),
        // K a bicycle kick that knocks down.
        20 => (5, 5, 22, 1250, 6, 22, 12, 40, 30, 0, Overhead, false),
        21 => (10, 6, 34, 1550, 12, 30, 16, 120, 60, 0, Overhead, true),
        // Direction + button on the ground (toward / away from the opponent):
        // forward J a lunging hook, plus on block (pressure); back J a rear-hand
        // uppercut, an anti-air (a body in the air is launched).
        22 => (9, 3, 26, 1100, 9, 26, 17, 70, 30, 0, Mid, false),
        23 => (10, 4, 34, 1150, 9, 28, 12, 80, 30, 0, Mid, false),
        // Forward U an advancing roundhouse that covers ground (`advance`);
        // back U a thrust kick that pushes the opponent away.
        24 => (15, 4, 36, 1650, 12, 30, 14, 120, 90, 0, Mid, false),
        25 => (12, 4, 32, 1700, 9, 26, 17, 90, 200, 0, Mid, false),
        // Forward K a flying knee: from above (beats a crouching guard),
        // launches, very unsafe.
        26 => (17, 4, 46, 1300, 13, 36, 14, 160, 40, 80, Overhead, true),
        // Forward + special: a hurricane kick across the room (500 meter).
        27 => (14, 10, 50, 1500, 18, 34, 22, 150, 160, 70, Mid, true),
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
/// Fighting styles: the same moves tuned for a different plan (R14 in
/// docs/COMBAT_RESEARCH.md). All-round: the base table. Pressure: closes in
/// fast and wins up close (quicker jab, hook and low kick, a plus-ish cross,
/// a longer throw, a short burst special) but its kicks are short. Range:
/// holds the opponent at the end of its kicks (longer kicks and special,
/// quicker retreat) but is slower up close.
pub const ALLROUND: u32 = 0;
pub const PRESSURE: u32 = 1;
pub const RANGE: u32 = 2;

/// `attack` as a fighter of `style` performs it.
pub fn attack_for(style: u32, action: u32) -> Option<Move> {
    let mut m = attack(action)?;
    match (style, action) {
        (PRESSURE, 1) => {
            m.startup -= 1;
            m.total -= 1;
            m.damage += 1;
        }
        (PRESSURE, 11) => {
            m.blockstun += 2;
            m.damage += 1;
        }
        (PRESSURE, 16 | 17) => {
            m.startup -= 1;
            m.total -= 1;
            m.damage += 1;
        }
        (PRESSURE, 4) => {
            m.reach += 120;
            m.damage += 2;
        }
        (PRESSURE, 8 | 12 | 18) => m.reach -= 40,
        // A short, quick burst instead of the long push.
        (PRESSURE, 14) => {
            m.reach = 1650;
            m.startup = 14;
            m.total = 42;
            m.push = 130;
        }
        (RANGE, 18) => m.reach += 80,
        (RANGE, 12) => m.reach += 60,
        // A long reach for the special: it covers half a room.
        (RANGE, 14) => {
            m.reach = 2450;
            m.startup = 20;
            m.total = 50;
        }
        (RANGE, 1 | 17) => {
            m.startup += 1;
            m.total += 1;
        }
        (RANGE, 11) => m.blockstun -= 1,
        (RANGE, 4) => m.reach -= 100,
        _ => {}
    }
    Some(m)
}

/// Walking speed of a style (mm/tick): forward, back.
pub fn walk(style: u32) -> (i32, i32) {
    match style {
        PRESSURE => (36, 22),
        RANGE => (27, 27),
        _ => (30, 24),
    }
}

/// Running speed of a style (mm/tick, forward; `RUN` held). Backing off
/// with `RUN` is half again the walk back.
pub fn run(style: u32) -> i32 {
    match style {
        PRESSURE => 68,
        RANGE => 56,
        _ => 62,
    }
}

/// Jump attacks: any of them once per jump.
pub fn airborne(action: u32) -> bool {
    matches!(action, 13 | 20 | 21)
}

/// Moves that carry the body forward (mm on this frame of the move).
pub fn advance(action: u32, frame: u32) -> i32 {
    match (action, frame) {
        (24, 2..=13) => 32,
        (26, 3..=16) => 42,
        (27, 2..=30) => 50,
        _ => 0,
    }
}

/// The pressure style's forward dash can turn into an attack from this frame.
pub const DASH_CANCEL: u32 = 8;

/// Dash speed of a style (mm/tick over its first ten frames).
pub fn dash(style: u32, forward: bool) -> i32 {
    match (style, forward) {
        (PRESSURE, true) => 96,
        (RANGE, false) => 84,
        _ => 72,
    }
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
                | (22, 12)
                | (22, 10)
                | (23, 8)
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
