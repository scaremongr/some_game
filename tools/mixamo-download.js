// PULSE: экспорт анимаций и персонажей Mixamo.
// 1. Откройте https://www.mixamo.com, войдите и выберите основного персонажа
//    (того, на ком собраны анимации игры).
// 2. В терминале проекта запустите: python tools/mixamo-fetch.py
// 3. F12 → Console. Если Chrome попросит, введите: allow pasting
// 4. Вставьте весь этот файл и нажмите Enter. На вопрос о нескольких загрузках — «Разрешить».
// Страница Mixamo не даёт скриптам скачивать файлы из своего хранилища (CSP),
// поэтому скрипт только экспортирует и сохраняет в «Загрузки» списки временных
// ссылок pulse-mixamo-urls-NN.json; сами FBX забирает tools/mixamo-fetch.py.
// Токен входа не покидает браузер: в файлах только ссылки на клипы и модели.
//
// FROM — первый номер клипа из списка, который нужно экспортировать (более
// ранние уже скачаны). FIGHTERS — части имён персонажей Mixamo: каждый
// найденный экспортируется в T-позе со скином. Весь каталог персонажей
// сохраняется в pulse-mixamo-characters.json.
(async () => {
  // Round 3 (2026-10-06): the list and the fighters are done; only SEARCH runs.
  const FROM = 1e9;
  const FIGHTERS = [];
  // SEARCH: [query, title pattern, how many] — clips found by name in the
  // Mixamo catalogue and numbered from NEXT on (runs, a slide, jump attacks).
  const NEXT = 180;
  const SEARCH = [
    ['run', /^(running|fast run|standard run|run forward|sprint|jog forward|running backward|run backward)$/i, 6],
    ['sprint', /sprint|fast run/i, 2],
    ['slide', /slide/i, 3],
    ['jump kick', /kick/i, 3],
    ['jump punch', /punch/i, 2],
  ];
  const LIST = [["c9ca582c-b96c-11e4-a802-0aaa78deedf9", "Bouncing Fight Idle With Guard Up"], ["c9c70517-b96c-11e4-a802-0aaa78deedf9", "Boxing Idle"], ["c9cadd1a-b96c-11e4-a802-0aaa78deedf9", "Bouncing Boxing Idle"], ["c9c63ce7-b96c-11e4-a802-0aaa78deedf9", "Male Fight Idle Boxing Stance"], ["c9cd050b-b96c-11e4-a802-0aaa78deedf9", "Mma Standing Idle"], ["c9c60b71-b96c-11e4-a802-0aaa78deedf9", "Male Boxing Idle"], ["c9c8f0d9-b96c-11e4-a802-0aaa78deedf9", "Standing Into Fighting Stance From Crouching"], ["c9cce7b7-b96c-11e4-a802-0aaa78deedf9", "Transition From Standing Idle To Fight Idle"], ["c9cb50af-b96c-11e4-a802-0aaa78deedf9", "Short Boxing Step Forward"], ["c9cb1b52-b96c-11e4-a802-0aaa78deedf9", "Medium Boxing Step Forward"], ["c9caebc0-b96c-11e4-a802-0aaa78deedf9", "Long Boxing Step Forward"], ["c9cadb92-b96c-11e4-a802-0aaa78deedf9", "Short Boxing Step Backward"], ["c9cadc5e-b96c-11e4-a802-0aaa78deedf9", "Medium Boxing Step Backward"], ["c9cb4f35-b96c-11e4-a802-0aaa78deedf9", "Long Boxing Step Backward"], ["c9c720b7-b96c-11e4-a802-0aaa78deedf9", "Boxing Advancing Forward"], ["c9c61914-b96c-11e4-a802-0aaa78deedf9", "Boxing Dodge Advance"], ["c9c619e3-b96c-11e4-a802-0aaa78deedf9", "Boxing Dodge Retreat"], ["c9c71c43-b96c-11e4-a802-0aaa78deedf9", "Boxing Leading Hand Jab"], ["c9c5f7e1-b96c-11e4-a802-0aaa78deedf9", "Jab Punch"], ["c9cb1733-b96c-11e4-a802-0aaa78deedf9", "Short Head Jab"], ["c9cb19f9-b96c-11e4-a802-0aaa78deedf9", "Med Head Jab"], ["c9cb1868-b96c-11e4-a802-0aaa78deedf9", "Long Head Jab"], ["c9cb144f-b96c-11e4-a802-0aaa78deedf9", "Short Body Jab"], ["c9cb1672-b96c-11e4-a802-0aaa78deedf9", "Mid Body Jab"], ["c9cb1594-b96c-11e4-a802-0aaa78deedf9", "Long Body Jab"], ["c9c5f8da-b96c-11e4-a802-0aaa78deedf9", "Cross Punch"], ["c9c7f89a-b96c-11e4-a802-0aaa78deedf9", "A Cross Punch"], ["c9c7084d-b96c-11e4-a802-0aaa78deedf9", "Back Hand Cross"], ["c9caec94-b96c-11e4-a802-0aaa78deedf9", "Short Jab Cross"], ["c9c7060b-b96c-11e4-a802-0aaa78deedf9", "Boxing Jab Cross Combo"], ["c9cae4da-b96c-11e4-a802-0aaa78deedf9", "Boxing Jab Cross Medium"], ["c9cae414-b96c-11e4-a802-0aaa78deedf9", "Boxing Jab Cross Long"], ["c9c717c6-b96c-11e4-a802-0aaa78deedf9", "Boxing Lead Hand Hook"], ["c9c70b4b-b96c-11e4-a802-0aaa78deedf9", "Boxing Back Hand Hook"], ["c9c7fb71-b96c-11e4-a802-0aaa78deedf9", "A Hook Punch"], ["c9c5fab8-b96c-11e4-a802-0aaa78deedf9", "Hook Punch With The Rear Hand"], ["c9cb1065-b96c-11e4-a802-0aaa78deedf9", "Short Hook Punch To The Head"], ["c9cb1308-b96c-11e4-a802-0aaa78deedf9", "Mid Hook Punch To The Head"], ["c9cb11ba-b96c-11e4-a802-0aaa78deedf9", "Long Hook Punch To The Head"], ["c9c71880-b96c-11e4-a802-0aaa78deedf9", "Boxing Lead Hand Uppercut"], ["c9c70319-b96c-11e4-a802-0aaa78deedf9", "Boxing Back Hand Uppercut"], ["c9c9c626-b96c-11e4-a802-0aaa78deedf9", "Standing Left Uppercut Punch"], ["8202fbeb-5755-4943-ba48-c7d8c8af9407", "Exaggerated Punch With Right Hand"], ["c9c9b7c5-b96c-11e4-a802-0aaa78deedf9", "Right Hook Punch From Idle"], ["c9ccb404-b96c-11e4-a802-0aaa78deedf9", "Quick Left Handed Punch"], ["c9caf4f2-b96c-11e4-a802-0aaa78deedf9", "Four Punch Combo"], ["c9c7f1df-b96c-11e4-a802-0aaa78deedf9", "Quad Punch Combo"], ["c9caf5b4-b96c-11e4-a802-0aaa78deedf9", "Eight Punch Combo"], ["c9cadde1-b96c-11e4-a802-0aaa78deedf9", "Illegal Elbow To The Head"], ["c9cadea3-b96c-11e4-a802-0aaa78deedf9", "Illegal Elbow Uppercut"], ["c9c61b83-b96c-11e4-a802-0aaa78deedf9", "Male Elbow Punch"], ["c9c69cbf-b96c-11e4-a802-0aaa78deedf9", "Street Fighter Hadouken"], ["c9c7aa9c-b96c-11e4-a802-0aaa78deedf9", "Uppercut Jab And Open Palm Strike Combo"], ["c9cd39f5-b96c-11e4-a802-0aaa78deedf9", "Mma High Kick"], ["c9cd28fc-b96c-11e4-a802-0aaa78deedf9", "Mma Low Kick"], ["c9cd3af5-b96c-11e4-a802-0aaa78deedf9", "Mma Roundhouse Kick"], ["c9cd278a-b96c-11e4-a802-0aaa78deedf9", "Mma Side Kick"], ["c9cd2845-b96c-11e4-a802-0aaa78deedf9", "Mma Spinning Back Kick"], ["c9c5fba3-b96c-11e4-a802-0aaa78deedf9", "Roundhouse Kick With The Rear Foot"], ["c9c63db2-b96c-11e4-a802-0aaa78deedf9", "Roundhouse Kick With Front Foot Advancing"], ["c9c7fc2e-b96c-11e4-a802-0aaa78deedf9", "A Roundhouse Kick To The Side Of An Opponent"], ["c9c61f91-b96c-11e4-a802-0aaa78deedf9", "Male Front Snap Kick With The Lead Foot"], ["c9c60c4f-b96c-11e4-a802-0aaa78deedf9", "Kicking With Lead Foot"], ["c9c63f53-b96c-11e4-a802-0aaa78deedf9", "Male Thrust Kick With The Rear Foot"], ["c9c7f59a-b96c-11e4-a802-0aaa78deedf9", "Side Kick"], ["c9c62472-b96c-11e4-a802-0aaa78deedf9", "Double Front Snap Kick"], ["c9c6436d-b96c-11e4-a802-0aaa78deedf9", "Front Leg Sweep"], ["c9c64507-b96c-11e4-a802-0aaa78deedf9", "Back Leg Sweep"], ["c9c7abe7-b96c-11e4-a802-0aaa78deedf9", "360 Leg Sweep Kick"], ["c9c71cfd-b96c-11e4-a802-0aaa78deedf9", "Flying Hurricane Kick"], ["c9c72d42-b96c-11e4-a802-0aaa78deedf9", "Flying Bicycle Kick"], ["c9c62882-b96c-11e4-a802-0aaa78deedf9", "Butterfly Kick Advancing"], ["c9cae0e3-b96c-11e4-a802-0aaa78deedf9", "Muay Thai Illegal Knee"], ["c9cae022-b96c-11e4-a802-0aaa78deedf9", "Boxing Illegal Knee"], ["c9c70c09-b96c-11e4-a802-0aaa78deedf9", "Boxing Illegal Knee"], ["c9ca9a5a-b96c-11e4-a802-0aaa78deedf9", "Jumping Knee Followed By A Punch"], ["c9cbb798-b96c-11e4-a802-0aaa78deedf9", "Capoeira High Kick"], ["c9cba193-b96c-11e4-a802-0aaa78deedf9", "Capoeira Thrust Kick"], ["c9c7a795-b96c-11e4-a802-0aaa78deedf9", "Throwing Opponent Over The Shoulder With A Leg Hook"], ["c9c7a914-b96c-11e4-a802-0aaa78deedf9", "Thrown Over The Shoulder With A Leg Hook By An Aggressor"], ["c9ca9b8a-b96c-11e4-a802-0aaa78deedf9", "Grabbing And Slamming Someone To The Ground"], ["c9c7a9d4-b96c-11e4-a802-0aaa78deedf9", "Hell Slammer Attacker"], ["c9c7f035-b96c-11e4-a802-0aaa78deedf9", "Hell Slammer Victim"], ["c9cab0fc-b96c-11e4-a802-0aaa78deedf9", "Lucha Libre Style Aerial Shoulder Throw"], ["c9c72233-b96c-11e4-a802-0aaa78deedf9", "Blocking Body With Arms"], ["c9caee1d-b96c-11e4-a802-0aaa78deedf9", "High Center Block"], ["c9caf317-b96c-11e4-a802-0aaa78deedf9", "Low Center Block"], ["03aca155-0078-40a3-a52b-04d3eaa7287d", "Block Reaction"], ["c9c687a7-b96c-11e4-a802-0aaa78deedf9", "Standing Reaction To Shove"], ["6b24d443-6dc6-444b-80a9-3a6867725cd7", "Transition From Standing Idle To Block Idle"], ["c9cb5867-b96c-11e4-a802-0aaa78deedf9", "Receiving A Light Hit To The Head From A Straight Punch"], ["c9cb52f3-b96c-11e4-a802-0aaa78deedf9", "Receiving A Light Hit To The Head From A Left Punch"], ["c9cb5616-b96c-11e4-a802-0aaa78deedf9", "Receiving A Light Hit To The Head From A Right Punch"], ["c9cb57a9-b96c-11e4-a802-0aaa78deedf9", "Receiving A Medium Hit To The Head From A Straight Punch"], ["c9cb5234-b96c-11e4-a802-0aaa78deedf9", "Receiving A Medium Hit To The Head From A Left Punch"], ["c9cb546f-b96c-11e4-a802-0aaa78deedf9", "Receiving A Medium Hit To The Head From A Right Punch"], ["c9cb56e9-b96c-11e4-a802-0aaa78deedf9", "Receiving A Big Hit To The Head From A Straight Punch"], ["c9cb516e-b96c-11e4-a802-0aaa78deedf9", "Receiving A Big Hit To The Head From A Left Punch"], ["c9cb53b0-b96c-11e4-a802-0aaa78deedf9", "Receiving A Big Hit To The Head From A Right Punch"], ["c9cafd3d-b96c-11e4-a802-0aaa78deedf9", "Receiving A Hit In The Stomach"], ["c9cafc81-b96c-11e4-a802-0aaa78deedf9", "Receiving A Big Hit In The Stomach"], ["c9c89ed9-b96c-11e4-a802-0aaa78deedf9", "Receive Punch To The Body"], ["c9c8976c-b96c-11e4-a802-0aaa78deedf9", "Receive Stomach Uppercut"], ["c9c89bdf-b96c-11e4-a802-0aaa78deedf9", "Receive Stomach Uppercut"], ["c9c90592-b96c-11e4-a802-0aaa78deedf9", "Receive An Uppercut Punch To The Face"], ["c9cb0936-b96c-11e4-a802-0aaa78deedf9", "Getting Hit By An Uppercut"], ["c9cb0870-b96c-11e4-a802-0aaa78deedf9", "Getting Rocked By A Big Uppercut"], ["3faceec0-f022-4e81-8da2-5b0e38a66087", "Small Hit Reaction From The Front"], ["c9c947d0-b96c-11e4-a802-0aaa78deedf9", "Hit Reaction"], ["c9c82f93-b96c-11e4-a802-0aaa78deedf9", "Getting Hit In The Face From Various Angles"], ["c9c69efe-b96c-11e4-a802-0aaa78deedf9", "Rocking Back And Forth As If Dizzy"], ["c9c725f7-b96c-11e4-a802-0aaa78deedf9", "Knocked Out Falling To Back"], ["c9c8a565-b96c-11e4-a802-0aaa78deedf9", "Knocked Down To Stomach"], ["c9ca7f74-b96c-11e4-a802-0aaa78deedf9", "Male Knocked Down From A Punch"], ["c9c91079-b96c-11e4-a802-0aaa78deedf9", "Knocked Over And Falling To The Ground"], ["c9c7f722-b96c-11e4-a802-0aaa78deedf9", "Getting Feet Swept Out From Underneath And Falling"], ["c9cacc74-b96c-11e4-a802-0aaa78deedf9", "Getting Hit And Flipping Backwards"], ["c9cdb4c8-b96c-11e4-a802-0aaa78deedf9", "Dying Flying Backward"], ["c9cc9aaa-b96c-11e4-a802-0aaa78deedf9", "Falling Back Death"], ["c9c8f331-b96c-11e4-a802-0aaa78deedf9", "Hit In The Shoulder And Falls To The Ground"], ["c9c7276e-b96c-11e4-a802-0aaa78deedf9", "Getting Up From Back"], ["c9c90823-b96c-11e4-a802-0aaa78deedf9", "Getting Up From Being Knocked Down On The Ground"], ["c9c71f39-b96c-11e4-a802-0aaa78deedf9", "Getting Up From Stomach"], ["c9ccd8eb-b96c-11e4-a802-0aaa78deedf9", "Standing Up From A Soccer Fall"], ["c9c97399-b96c-11e4-a802-0aaa78deedf9", "Jumping In Place"], ["c9c97f8f-b96c-11e4-a802-0aaa78deedf9", "Jumping In Place"], ["c9c92527-b96c-11e4-a802-0aaa78deedf9", "Standing Jumping In Place"], ["c9c7fab0-b96c-11e4-a802-0aaa78deedf9", "Jump Up"], ["c9c808ef-b96c-11e4-a802-0aaa78deedf9", "Landing From Jump"], ["c9c9ea58-b96c-11e4-a802-0aaa78deedf9", "Standing Backflip"], ["c9cab793-b96c-11e4-a802-0aaa78deedf9", "Big Front Flip"], ["c9c991b1-b96c-11e4-a802-0aaa78deedf9", "Mid-Air Falling Idle"], ["c9cb3a87-b96c-11e4-a802-0aaa78deedf9", "Victory From A Boxing Win"], ["c9cb3878-b96c-11e4-a802-0aaa78deedf9", "Defeat From A Boxing Loss"], ["c9cb3946-b96c-11e4-a802-0aaa78deedf9", "Boxing Taunt"], ["c9c71b7f-b96c-11e4-a802-0aaa78deedf9", "Taunting Throwing Arms Back"], ["c9c723ad-b96c-11e4-a802-0aaa78deedf9", "Rallying The Crowd To Make Them Cheer"], ["c9c70794-b96c-11e4-a802-0aaa78deedf9", "Showing Frustration After A Loss"], ["c284106c-698e-40df-acb0-f89e764e5ba0", "Crouch Idle"], ["c9cc08cf-b96c-11e4-a802-0aaa78deedf9", "Low Crouching Idle"], ["c4bfbde5-ccb9-4b2c-8856-2815f4a13f6d", "Walking Forward While Crouched"], ["328e4430-480a-49c2-8086-ef5f1b5a1beb", "Walking Backwards While Crouched"], ["a16517d1-cc83-4973-b3f3-bbecac3538e9", "Transition From Standing Idle To Crouch Idle"], ["400dc4b4-f030-4527-b4e5-5f59a8727d4c", "Transition From Crouch Idle To Standing Idle"], ["c9c877e6-b96c-11e4-a802-0aaa78deedf9", "Standing To Crouching Transition"], ["c9c8469a-b96c-11e4-a802-0aaa78deedf9", "Crouched Hiding To Ducking"], ["c9cca5cc-b96c-11e4-a802-0aaa78deedf9", "Hard Floor Stomp"], ["c9ccae10-b96c-11e4-a802-0aaa78deedf9", "Stomping Foot On Ground"], ["c9cba7ba-b96c-11e4-a802-0aaa78deedf9", "Capoeira Ground Spin Kick"], ["c9cb9d65-b96c-11e4-a802-0aaa78deedf9", "Capoeira Spin Kick"], ["c9cd423b-b96c-11e4-a802-0aaa78deedf9", "Flying Sidekick From A Run"], ["c9ca9f2f-b96c-11e4-a802-0aaa78deedf9", "Front Flip To Kick"], ["c9cd0754-b96c-11e4-a802-0aaa78deedf9", "Mma Takedown To Ground And Pound"], ["c9cc88df-b96c-11e4-a802-0aaa78deedf9", "Great Sword Crouching Block Idle"], ["c9cc5f7e-b96c-11e4-a802-0aaa78deedf9", "Sword And Shield Crouch Block Idle"], ["c9ce7f5b-b96c-11e4-a802-0aaa78deedf9", "Swing Backflip To Superhero Pose"], ["c9c714c2-b96c-11e4-a802-0aaa78deedf9", "Celebrating After A Win"], ["c9c6b803-b96c-11e4-a802-0aaa78deedf9", "Ready To Combat To Defensive Idle"], ["18b23688-f7fd-4653-9105-b2238f09c4c5", "Crouch Idle"], ["dbc327f5-1358-4c2a-8d00-4a377a9a7978", "Walking Backwards While Crouched"], ["a0db410a-4b07-46db-be54-c30cc0498452", "Transition From Standing Idle To Crouch Idle"]];
  const FPS = '30';
  const BATCH = 5;
  const token = localStorage.access_token;
  if (!token) { console.error('Нет входа на mixamo.com — войдите и запустите снова.'); return; }
  const headers = { Accept: 'application/json', 'Content-Type': 'application/json', Authorization: `Bearer ${token}`, 'X-Api-Key': 'mixamo2' };
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  // Mixamo limits request rate (HTTP 429): back off and retry.
  const api = async (path, options = {}) => {
    for (let attempt = 0; attempt < 9; attempt++) {
      const r = await fetch('https://www.mixamo.com/api/v1/' + path, { headers, ...options });
      if (r.status === 429) {
        const wait = Math.min(90, 6 * 2 ** attempt);
        console.log(`… Mixamo просит подождать ${wait} с`);
        await sleep(wait * 1000);
        continue;
      }
      if (!r.ok && r.status !== 202) throw Error(`${path}: HTTP ${r.status}`);
      return r.json();
    }
    throw Error(`${path}: слишком много запросов`);
  };
  let batch = 0;
  const save = (data, name) => { const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([JSON.stringify(data, null, 1)], { type: 'application/json' })); a.download = name; document.body.appendChild(a); a.click(); a.remove(); };
  const clean = s => s.replace(/[\/:*?"<>|]/g, '');
  const all = [], pending = [];
  const flush = () => { if (pending.length) save(pending.splice(0), `pulse-mixamo-urls-${String(batch++).padStart(2, '0')}.json`); };
  const last = {};
  // Exports on the server, waits for the job and records the download link.
  const exportJob = async (character, body, entry) => {
    await api('animations/export', { method: 'POST', body: JSON.stringify(body) });
    let msg = {};
    for (let t = 0; t < 200; t++) {
      await sleep(1500);
      msg = await api(`characters/${character}/monitor`);
      if (msg.status === 'failed') throw Error(msg.message || 'export failed');
      if (msg.status === 'completed' && msg.job_result && msg.job_result !== last[character]) break;
    }
    if (msg.status !== 'completed' || msg.job_result === last[character]) throw Error('timeout');
    last[character] = msg.job_result;
    const done = { ...entry, url: msg.job_result };
    all.push(done); pending.push(done);
    if (pending.length >= BATCH) flush();
    await sleep(2000);
  };

  const primary = await api('characters/primary');
  const character = primary.primary_character_id;
  const todo = LIST.map((clip, i) => [i, ...clip]).filter(([i]) => i >= FROM);
  console.log(`%cPULSE: основной персонаж «${primary.primary_character_name}», клипов: ${todo.length}`, 'color:#57f0ce;font-weight:bold');
  for (const [i, id, name] of todo) {
    const file = clean(`${String(i).padStart(3, '0')} ${name}`);
    try {
      const product = await api(`products/${id}?similar=0&character_id=${character}`);
      const g = product.details.gms_hash;
      const hash = { ...g, params: g.params.map(p => p[1]).join(','), overdrive: 0, trim: g.trim.map(t => Math.round(t)) };
      if ('inplace' in g) hash.inplace = true;
      await exportJob(character, { character_id: character, gms_hash: [hash], preferences: { format: 'fbx7_2019', skin: 'false', fps: FPS, reducekf: '0' }, product_name: file, type: 'Motion' },
        { kind: 'clip', index: i, id, name, file: file + '.fbx' });
      console.log(`✓ клип ${i} ${name}`);
    } catch (e) {
      all.push({ kind: 'clip', index: i, id, name, error: String(e) });
      console.warn(`✗ клип ${i} ${name}: ${e}`);
    }
  }

  // Clips found by name.
  let next = NEXT;
  const taken = new Set();
  for (const [query, pattern, count] of SEARCH) {
    let found = 0;
    for (let page = 1; page <= 3 && found < count; page++) {
      const data = await api(`products?page=${page}&limit=96&order=&type=Motion%2CMotionPack&query=${encodeURIComponent(query)}`);
      for (const r of data.results || []) {
        const name = r.name || r.description || '';
        if (found >= count || taken.has(r.id) || r.type !== 'Motion' || !pattern.test(name)) continue;
        taken.add(r.id);
        const i = next++, file = clean(`${String(i).padStart(3, '0')} ${name}`);
        try {
          const product = await api(`products/${r.id}?similar=0&character_id=${character}`);
          const g = product.details.gms_hash;
          const hash = { ...g, params: g.params.map(p => p[1]).join(','), overdrive: 0, trim: g.trim.map(t => Math.round(t)) };
          // Runs and slides keep their travel: the game plays them by distance.
          await exportJob(character, { character_id: character, gms_hash: [hash], preferences: { format: 'fbx7_2019', skin: 'false', fps: FPS, reducekf: '0' }, product_name: file, type: 'Motion' },
            { kind: 'clip', index: i, id: r.id, name, file: file + '.fbx' });
          found++;
          console.log(`✓ найден и выгружен ${i} ${name}`);
        } catch (e) {
          all.push({ kind: 'clip', index: i, id: r.id, name, error: String(e) });
          console.warn(`✗ ${name}: ${e}`);
        }
      }
      if (!(data.pagination?.num_pages > page)) break;
    }
    if (!found) console.log(`… по запросу «${query}» ничего не подошло`);
  }

  // Character catalogue, then every requested fighter in T-pose with its skin.
  const catalogue = [];
  if (FIGHTERS.length)
  for (let page = 1, pages = 1; page <= pages; page++) {
    const data = await api(`products?page=${page}&limit=96&order=&type=Character&query=`);
    pages = data.pagination?.num_pages || 1;
    for (const c of data.results || []) catalogue.push({ id: c.id, name: c.description || c.name, thumbnail: c.thumbnail || c.thumbnail_animated || null });
  }
  save(catalogue, 'pulse-mixamo-characters.json');
  console.log(`%cPULSE: персонажей в каталоге: ${catalogue.length}`, 'color:#57f0ce;font-weight:bold');
  const picked = [];
  for (const key of FIGHTERS) {
    const c = catalogue.find(c => c.name.toLowerCase().includes(key) && !picked.includes(c));
    if (c) picked.push(c); else console.log(`… нет персонажа «${key}»`);
  }
  for (const c of picked) {
    const file = clean(c.name);
    try {
      await exportJob(c.id, { character_id: c.id, product_name: file, type: 'Character', preferences: { format: 'fbx7_2019', mesh: 't-pose' }, gms_hash: null },
        { kind: 'fighter', dir: 'fighters', id: c.id, name: c.name, file: file + '.fbx' });
      console.log(`✓ персонаж ${c.name}`);
    } catch (e) {
      all.push({ kind: 'fighter', id: c.id, name: c.name, error: String(e) });
      console.warn(`✗ персонаж ${c.name}: ${e}`);
    }
  }
  flush();
  save({ character: primary, fps: FPS, clips: all }, 'pulse-mixamo-manifest.json');
  const failed = all.filter(m => m.error).length;
  console.log(`%cPULSE: готово. Экспортировано ${all.length - failed}, ошибок ${failed}.`, 'color:#57f0ce;font-weight:bold');
})();
