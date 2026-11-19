import assert from 'node:assert/strict';

export function installCombinedMounts() {
  window.__runCombinedMounts = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1], D = seats[3];
    if (B === game.me) game.swapPlayer(A);
    const seat = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    const tracked = new Map(), events = [], stages = [];
    let trial = null;
    const make = (id, name, suit, number) => { const card = game.createCard2(name, suit, number); tracked.set(card, id); return card; };
    const names = ['disableEquip', 'enableEquip', 'expandEquip', 'loseToDiscardpile', 'lose', 'equip', 'replaceEquip', 'chooseButton'];
    const timings = [...new Set(names.flatMap(name => ['', 'Before', 'Begin', 'End', 'After', 'Cancelled'].map(suffix => name + suffix)))];
    const ids = cards => (cards || []).flatMap(card => tracked.has(card) ? [tracked.get(card)] : (card.cards || []).filter(item => tracked.has(item)).map(item => tracked.get(item)));
    const zone = card => { const position = get.position(card, true), owner = get.owner(card); return owner && position ? `${seat(owner)}:${position}` : position ?? null; };
    const types = [3, 4, 'equip3_4'];
    const item = card => ({ ids: ids([card]), name: card.name, subtypes: get.subtypes(card, false) });
    const state = player => ({ enabled: types.map(type => player.countEnabledSlot(type)), disabled: types.map(type => player.countDisabledSlot(type)),
      empty: types.map(type => player.countEmptySlot(type)), equipable: types.map(type => player.countEquipableSlot(type)),
      horse: [player.hasEnabledSlot('horse'), player.hasEmptySlot('horse'), player.hasDisabledSlot('horse')],
      canEquip: ['chitu', 'jueying'].map(name => [player.canEquip(name, false), player.canEquip(name, true)]),
      distance: [get.distance(A, player), get.distance(player, A)],
      distanceToD: [get.distance(D, player), get.distance(player, D)],
      registered: player.getVEquips('equip3_4').map(item), physical: player.getCards('e').map(item),
      markers: [...player.node.equips.childNodes].filter(card => card.classList.contains('feichu')).map(card => card.name),
      cards: Object.fromEntries([...tracked].map(([card, id]) => [id, zone(card)])) });
    const capture = name => stages.push({ name, B: state(B), D: state(D) });
    function record(kind, timing, event) {
      if (!trial || !timings.includes(timing)) return;
      events.push([trial, kind, timing, event.name, event.getParent()?.name ?? null, seat(event.player), seat(event.source),
        event.slots?.slice() ?? null, ids(event.cards), ids(event.result?.links),
        event.dialog?.buttons?.map(button => ids([button.link])) ?? null, !!event.finished, !!event._cancelled, state(B)]);
    }
    const observer = 'reference_combined_mounts_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true,
      filter: () => !!trial, async content(event, trigger) { record('callback', event.triggername, trigger); } };
    game.finishSkill(observer); A.addSkill(observer);
    game.globalEventHandlers.pushHandler('chooseButton', (event, { state }) => {
      if (trial && state === 'begin' && (event.getParent('disableEquip', true) || event.getParent('replaceEquip', true))) {
        event.ai = button => ids([button.link]).includes('chitu') ? 100 : 0;
      }
    });
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return original.apply(this, arguments); };
    const saved = _status.mountCombined;
    const run = async (name, action) => { trial = name; await action(); capture(name); trial = null; };
    try {
      for (const player of seats) { const held = player.getCards('hej'); if (held.length) await player.discard(held); }
      _status.mountCombined = true;
      const chitu = make('chitu', 'chitu', 'heart', 5);
      const jueying = make('jueying', 'jueying', 'spade', 5);
      const dawan = make('dawan', 'dawan', 'spade', 13);
      capture('empty');
      await run('equip-chitu', () => B.equip(chitu));
      await run('equip-jueying', () => B.equip(jueying));
      await run('expand-3-4', () => B.expandEquip([3, 4]));
      await run('equip-chitu-again', () => B.equip(chitu));
      await run('equip-third', () => B.equip(dawan));
      await run('disable-3-3-4', () => B.disableEquip([3, 3, 4]));
      await run('disable-3-4-first', () => B.disableEquip([3, 4]));
      await run('disable-3-4-second', () => B.disableEquip([3, 4]));
      await run('enable-3-4-4', () => B.enableEquip([3, 4, 4]));
      await run('enable-4', () => B.enableEquip([4]));
      await run('expand-d-3-4-4', () => D.expandEquip([3, 4, 4]));
      return { combined: get.is.mountCombined(), columns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'slots', 'cards', 'chosen', 'candidates', 'finished', 'cancelled', 'state'], events, stages };
    } finally { _status.mountCombined = saved; proto.trigger = original; A.removeSkill(observer); }
  };
}
export function checkCombinedMounts(result) {
  assert.equal(result.combined, true);
  assert.deepEqual(result.stages.map(stage => stage.name), ['empty', 'equip-chitu', 'equip-jueying', 'expand-3-4', 'equip-chitu-again', 'equip-third', 'disable-3-3-4', 'disable-3-4-first', 'disable-3-4-second', 'enable-3-4-4', 'enable-4', 'expand-d-3-4-4']);
  const chosen = result.events.filter(row => row[0] === 'equip-third' && row[1] === 'request' && row[2] === 'chooseButtonAfter');
  assert.equal(chosen.length, 1);
  assert.deepEqual([chosen[0][4], chosen[0][5]], ['replaceEquip', 'B']);
  assert.deepEqual([chosen[0][9], chosen[0][10]], [['chitu'], [['jueying'], ['chitu']]]);
  const stage = name => result.stages.find(item => item.name === name);
  assert.deepEqual(stage('equip-chitu-again').B.registered.map(card => card.name), ['jueying', 'chitu']);
  assert.deepEqual([stage('disable-3-3-4').B.enabled, stage('disable-3-3-4').B.cards.jueying, stage('disable-3-3-4').B.cards.dawan], [[1, 0, 1], 'd', 'd']);
  assert.deepEqual([stage('disable-3-4-second').B.enabled, stage('disable-3-4-second').B.disabled], [[-1, 0, 1], [3, 0, 0]]);
  assert.deepEqual(stage('expand-d-3-4-4').D.enabled, [3, 0, 1]);
}
