window.arenaRenderBytes = null;
window.arenaModelStatus = 0;
miniquad_add_plugin({name:'pulse-arena',version:1,register_plugin(imports){
  imports.env.fight_model_status = status => { window.arenaModelStatus = status; };
  // Sides already showing the requested fighter: bit 0 left, bit 1 right.
  imports.env.fight_avatars = mask => { window.arenaAvatarsReady = mask; };
  // The baked apartment: 1 shown, -1 unavailable.
  imports.env.fight_room_status = status => { window.arenaRoomStatus = status; };
  // Optional screenshot camera: [eye x,y,z, target x,y,z, fov, preview],
  // optionally followed by [pack clip index, seconds] to show a captured clip
  // on the left fighter (clip review).
  imports.env.fight_debug_camera = function(ptr) {
    const c = window.arenaDebugCamera;
    if (!c || c.length < 8) return 0;
    const out = new Float32Array(wasm_memory.buffer, ptr, 10);
    out.set(c.slice(0, 10));
    if (c.length < 10) out.fill(-1, c.length, 10);
    return 1;
  };
  // Continuous render tick for smooth interpolation (negative: newest state).
  imports.env.fight_clock = function() {
    return typeof window.arenaClock === 'function' ? window.arenaClock() : -1;
  };
  // Screen band left free by HUD and touch controls: [top, bottom] in 0..1.
  imports.env.fight_layout = function(ptr) {
    const band = window.arenaLayout;
    if (!band || band.length < 2) return 0;
    new Float32Array(wasm_memory.buffer, ptr, 2).set(band);
    return 1;
  };
  // Fighter bodies per side (window.arenaFighters: [{model, pack}, {model, pack}])
  // as UTF-8 lines: model, pack, model, pack.
  let fightersRef = null, fightersBytes = null;
  imports.env.fight_fighters = function(ptr, capacity) {
    const f = window.arenaFighters;
    if (!f || f.length < 2) return 0;
    if (f !== fightersRef) { fightersRef = f; fightersBytes = new TextEncoder().encode(f.map(x => x.model + '\n' + x.pack).join('\n')); }
    if (fightersBytes.length > capacity) return 0;
    new Uint8Array(wasm_memory.buffer, ptr, fightersBytes.length).set(fightersBytes);
    return fightersBytes.length;
  };
  imports.env.fight_read = function(ptr, capacity) {
    const bytes = window.arenaRenderBytes;
    if (!bytes || bytes.length > capacity) return 0;
    new Uint8Array(wasm_memory.buffer,ptr,bytes.length).set(bytes);
    return bytes.length;
  };
}});
