// Online prediction (rollback) for a server-authoritative fight.
//
// The server owns the fight; the page runs the same simulation a few ticks
// ahead of it, so a press shows at once instead of after a round trip. Each
// snapshot is loaded and the presses the server has not applied yet are
// replayed on top; the opponent is assumed to keep holding what they held.
// The clock is tuned so that an input made for tick t reaches the server
// just before tick t: the server echoes the tick each player's last input
// was applied at. Plain logic without the page, so tests can drive it.
import { simulation } from './combat.js';

export const STEP = 1000 / 60;
/// Never more than this many ticks ahead of a snapshot (very slow links
/// fall back to visible corrections rather than replaying seconds).
const LEAD_MAX = 30;
/// Ticks of slack between an input's arrival and the tick it was made for.
const SLACK = 1;

export class Prediction {
  constructor(module, side, snapshot, now) {
    this.sim = simulation(module);
    this.side = side;
    this.history = new Map();
    this.sent = new Map();
    this.tick = snapshot.tick;
    this.base = snapshot.tick;
    // Start a few ticks ahead; the echoes tune it within a second.
    this.offset = snapshot.tick + 4 - now / STEP;
    this.stats = { snapshots: 0, corrections: 0, lead: 0, error: 0 };
    this.load(snapshot, now, null);
  }

  /// A server snapshot `m` (`state`, `ack`, `arrived`). Returns false when
  /// it belongs to another match (the caller starts a new prediction).
  snapshot(m, now) {
    const snap = m.state;
    if (snap.tick < this.base) return false;
    const mine = m.ack?.[this.side], applied = m.arrived?.[this.side];
    const made = this.sent.get(mine);
    if (made !== undefined && applied > 0) {
      const error = made - applied - SLACK;
      this.offset -= Math.max(-0.5, Math.min(0.5, error * 0.2));
      this.stats.error = this.stats.error * 0.9 + error * 0.1;
    }
    for (const k of this.sent.keys()) if (k <= mine) this.sent.delete(k);
    this.load(snap, now, this.tick);
    return true;
  }

  load(snap, now, until) {
    for (const t of this.history.keys()) if (t <= snap.tick) this.history.delete(t);
    const guess = until > snap.tick ? this.sim.state().fighters : null;
    this.sim.load(snap);
    this.tick = this.base = snap.tick;
    this.held = snap.fighters[1 - this.side].previous;
    this.authority = snap;
    while (until !== null && this.tick < until) this.step(this.history.get(this.tick + 1) ?? 0);
    const lead = this.offset + now / STEP - snap.tick;
    if (lead > LEAD_MAX) this.offset -= lead - LEAD_MAX;
    if (lead < 1) this.offset += 1 - lead;
    this.stats.snapshots++;
    this.stats.lead = this.offset + now / STEP - snap.tick;
    if (guess) {
      const fixed = this.sim.state().fighters;
      if (fixed.some((f, i) => f.action !== guess[i].action || Math.abs(f.x - guess[i].x) > 30 || f.hp !== guess[i].hp)) this.stats.corrections++;
    }
  }

  step(mine) {
    const inputs = this.side === 0 ? [mine, this.held] : [this.held, mine];
    this.sim.step(inputs[0], inputs[1]);
    this.tick++;
  }

  /// Steps up to the clock with the player's input: `held` bits, plus
  /// `edges` pressed since the last call (they go into the first step).
  /// Returns the number of steps taken.
  advance(now, held, edges = 0) {
    const target = Math.floor(now / STEP + this.offset);
    let steps = 0;
    while (this.tick < target && steps < LEAD_MAX) {
      const mine = held | (steps === 0 ? edges : 0);
      this.history.set(this.tick + 1, mine);
      this.step(mine);
      steps++;
    }
    // A long stall (a hidden tab): skip ahead instead of racing.
    if (this.tick < target - LEAD_MAX) this.offset -= target - this.tick;
    return steps;
  }

  /// An input message `seq` is being sent now: it is meant for the next tick.
  sending(seq) {
    this.sent.set(seq, this.tick + 1);
  }

  state() {
    return this.sim.state();
  }
}
