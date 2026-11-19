import assert from 'node:assert/strict';

export function installEquipSlots() {
  window.__runEquipSlots = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1], D = seats[3];
    if (B === game.me) game.swapPlayer(A);
    const seat = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    const tracked = new Map(), events = [], stages = [];
    let trial = null, cancel = null;
    const types = [1, 2, 3, 4, 5];
    const make = (id, name, suit, number) => { const card = game.createCard2(name, suit, number); tracked.set(card, id); return card; };
    const names = ['disableEquip', 'enableEquip', 'expandEquip', 'loseToDiscardpile', 'lose', 'equip', 'chooseButton'];
    const timings = [...new Set(names.flatMap(name => ['', 'Before', 'Begin', 'End', 'After', 'Cancelled'].map(suffix => name + suffix)))];
    const zone = card => { const position = get.position(card, true), owner = get.owner(card);
      return owner && position ? `${seat(owner)}:${position}` : position ?? null; };
    const ids = cards => (cards || []).flatMap(card => tracked.has(card) ? [tracked.get(card)]
      : (card.cards || []).filter(item => tracked.has(item)).map(item => tracked.get(item)));
    const state = player => ({ enabled: types.map(type => player.countEnabledSlot(type)),
      empty: types.map(type => player.countEmptySlot(type)),
      equipable: types.map(type => player.countEquipableSlot(type)),
      disabled: types.map(type => player.countDisabledSlot(type)),
      horse: [player.hasEnabledSlot('horse'), player.hasEmptySlot('horse'), player.hasDisabledSlot('horse')],
      equipment: ids(player.getCards('e')),
      markers: [...player.node.equips.childNodes].filter(card => card.classList.contains('feichu')).map(card => card.name),
      cards: Object.fromEntries([...tracked].map(([card, id]) => [id, zone(card)])) });
    const capture = name => stages.push({ name, ...state(B), canEquipWeapon: [B.canEquip('zhuge', false), B.canEquip('zhuge', true)] });
    function record(kind, timing, event) {
      if (!trial || !timings.includes(timing)) return;
      events.push([trial, kind, timing, event.name, event.getParent()?.name ?? null, seat(event.player), seat(event.source),
        event.slots?.slice() ?? null, ids(event.cards), ids(event.result?.links), !!event.finished, !!event._cancelled, state(B)]);
    }
    const observer = 'reference_equip_slots_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true,
      filter: () => !!trial,
      async content(event, trigger) {
        record('callback', event.triggername, trigger);
        if (event.triggername === cancel && trigger.player === B && trigger.getParent('disableEquip', true)) trigger.cancel();
      } };
    game.finishSkill(observer); A.addSkill(observer);
    game.globalEventHandlers.pushHandler('chooseButton', (event, { state }) => {
      if (trial && state === 'begin' && (event.getParent('disableEquip', true) || event.getParent('replaceEquip', true))) {
        event.ai = button => tracked.get(button.link) === 'weapon-1' ? 100 : 0;
      }
    });
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return original.apply(this, arguments); };
    const run = async (name, action) => { trial = name; await action(); capture(name); trial = null; };
    try {
      for (const player of seats) {
        const held = player.getCards('hej');
        if (held.length) await player.discard(held);
      }
      const weapon1 = make('weapon-1', 'zhuge', 'club', 1);
      const weapon2 = make('weapon-2', 'qinggang', 'spade', 6);
      const weapon3 = make('weapon-3', 'qinglong', 'spade', 5);
      const armor = make('armor', 'bagua', 'spade', 2);
      const horse = make('horse', 'jueying', 'spade', 5);
      const combined = get.is.mountCombined();
      capture('empty-default');
      await B.equip(weapon1); await B.equip(armor); await B.equip(horse);
      capture('equipped-default');
      await run('disable-weapon', () => B.disableEquip(1));
      await run('disable-weapon-repeat', () => B.disableEquip(1));
      await run('enable-weapon', () => B.enableEquip(1));
      await run('disable-armor-duplicate', () => B.disableEquip([2, 2]));
      await run('enable-armor-duplicate', () => B.enableEquip([2, 2]));
      await run('disable-empty-treasure', () => B.disableEquip(5));
      await run('enable-empty-treasure', () => B.enableEquip(5));
      await run('expand-weapon', () => B.expandEquip(1));
      await B.equip(weapon1); await B.equip(weapon2);
      capture('two-weapons');
      await run('disable-one-of-two', () => B.disableEquip(1));
      await run('disable-second', () => B.disableEquip(1));
      await run('enable-two', () => B.enableEquip([1, 1]));
      await B.equip(weapon1); await B.equip(weapon2);
      capture('re-equipped-two');
      await run('disable-two-duplicate', () => B.disableEquip([1, 1]));
      await run('enable-two-duplicate', () => B.enableEquip([1, 1]));
      await B.equip(weapon1); await B.equip(weapon2);
      await run('equip-third', () => B.equip(weapon3));

      // Separate trials show what survives cancellation of either wrapper.
      const cancellations = [];
      for (const timing of ['loseToDiscardpileBefore', 'loseBefore']) {
        if (B.countDisabledSlot(2)) await B.enableEquip(2);
        await B.equip(armor);
        trial = `cancel:${timing}`; cancel = timing;
        await B.disableEquip(2);
        cancellations.push({ timing, ...state(B) });
        cancel = trial = null;
        await B.enableEquip(2);
      }
      const mountSetting = _status.mountCombined;
      const mountDefault = state(D);
      _status.mountCombined = !combined;
      const mountAlternate = { combined: get.is.mountCombined(), ...state(D) };
      await D.disableEquip([3, 4]);
      const mountAlternateDisabled = state(D);
      await D.enableEquip([3, 4]);
      _status.mountCombined = mountSetting;
      return { combined, mountDefault, mountAlternate, mountAlternateDisabled,
        columns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'slots', 'cards', 'chosen',
          'finished', 'cancelled', 'state'], events, stages, cancellations };
    } finally { proto.trigger = original; A.removeSkill(observer); }
  };
}

export function checkEquipSlots(result) {
  assert.equal(result.combined, false);
  const stage = name => result.stages.find(item => item.name === name);
  assert.deepEqual([stage('empty-default').enabled, stage('empty-default').empty, stage('empty-default').equipable],
    [[1, 1, 1, 1, 1], [1, 1, 1, 1, 1], [1, 1, 1, 1, 1]]);
  assert.deepEqual([stage('equipped-default').enabled, stage('equipped-default').empty,
    stage('equipped-default').equipable, stage('equipped-default').canEquipWeapon],
  [[1, 1, 1, 1, 1], [0, 0, 0, 1, 1], [1, 1, 1, 1, 1], [false, true]]);
  for (const name of ['disable-weapon', 'disable-weapon-repeat']) {
    assert.deepEqual([stage(name).enabled[0], stage(name).disabled[0], stage(name).markers, stage(name).cards['weapon-1']],
      [0, 1, ['feichu_equip1'], 'd']);
  }
  assert.deepEqual([stage('disable-armor-duplicate').disabled[1], stage('disable-armor-duplicate').markers,
    stage('enable-armor-duplicate').enabled[1]], [1, ['feichu_equip2'], 1]);
  assert.deepEqual([stage('disable-empty-treasure').disabled[4], stage('disable-empty-treasure').markers,
    stage('enable-empty-treasure').enabled[4]], [1, ['feichu_equip5'], 1]);
  assert.deepEqual([stage('expand-weapon').enabled[0], stage('two-weapons').equipment.slice(0, 2)],
    [2, ['weapon-2', 'weapon-1']]);
  assert.deepEqual([stage('disable-one-of-two').enabled[0], stage('disable-one-of-two').cards['weapon-1'],
    stage('disable-one-of-two').cards['weapon-2']], [1, 'd', 'B:e']);
  assert.deepEqual([stage('disable-second').disabled[0], stage('disable-second').markers.length], [2, 2]);
  assert.deepEqual([stage('disable-two-duplicate').disabled[0], stage('disable-two-duplicate').markers.length,
    stage('disable-two-duplicate').cards['weapon-1'], stage('disable-two-duplicate').cards['weapon-2']], [2, 2, 'd', 'd']);
  assert.deepEqual([stage('equip-third').cards['weapon-1'], stage('equip-third').cards['weapon-2'],
    stage('equip-third').cards['weapon-3']], ['d', 'B:e', 'B:e']);
  for (const trial of result.cancellations) {
    assert.deepEqual([trial.disabled[1], trial.markers, trial.cards.armor], [1, ['feichu_equip2'], 'B:e']);
  }
  assert.deepEqual([result.mountAlternate.combined, result.mountAlternate.enabled, result.mountAlternateDisabled.enabled,
    result.mountAlternateDisabled.disabled, result.mountAlternateDisabled.markers],
  [true, [1, 1, 1, 0, 1], [1, 1, 0, 0, 1], [0, 0, 1, 0, 0], ['feichu_equip3']]);
  for (const name of [...new Set(result.events.map(row => row[0]))]) {
    const rows = result.events.filter(row => row[0] === name);
    assert.deepEqual(rows.filter(row => row[1] === 'request').map(row => row[2]),
      rows.filter(row => row[1] === 'callback').map(row => row[2]), name);
  }
  const request = name => result.events.filter(row => row[0] === name && row[1] === 'request');
  const picked = request('disable-one-of-two').find(row => row[2] === 'chooseButtonAfter');
  assert.deepEqual([picked[5], picked[4], picked[9]], ['A', 'disableEquip', ['weapon-1']]);
  const replaced = request('equip-third').find(row => row[2] === 'chooseButtonAfter');
  assert.deepEqual([replaced[5], replaced[4], replaced[9]], ['B', 'replaceEquip', ['weapon-1']]);
  assert.ok(request('cancel:loseToDiscardpileBefore').some(row => row[2] === 'loseToDiscardpileCancelled'));
  assert.ok(request('cancel:loseBefore').some(row => row[2] === 'loseCancelled'));
}
