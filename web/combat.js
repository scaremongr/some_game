// The exact same WASM module runs training in the browser and authoritative PvP in Node.
export function simulation(module, seed = 1) {
  const { exports: api } = new WebAssembly.Instance(module, {});
  const decoder = new TextDecoder();
  api.arena_reset(seed);
  return {
    step(a, b) { api.arena_step(a, b); },
    bot(side) { return api.arena_bot(side); },
    forfeit(side) { api.arena_forfeit(side); },
    tick() { return api.arena_tick(); },
    // Replaces the match with a snapshot (client prediction rolls back to it).
    load(state) {
      const bytes = new TextEncoder().encode(typeof state === 'string' ? state : JSON.stringify(state));
      new Uint8Array(api.memory.buffer, api.arena_alloc(bytes.length), bytes.length).set(bytes);
      return api.arena_load(bytes.length) === 1;
    },
    state() {
      const ptr = api.arena_state();
      return JSON.parse(decoder.decode(new Uint8Array(api.memory.buffer, ptr, api.arena_state_len())));
    },
  };
}
