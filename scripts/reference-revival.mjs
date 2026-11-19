import assert from 'node:assert/strict';

// Fixture operations call the pinned upstream death and revival methods.
export function installReferenceRevival() {
  window.__runReferenceRevival = async ({ lib, game, ui, get, _status, actor: A }) => {
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    if (new Set(seats).size !== 8 || seats.at(-1).nextSeat !== A) throw new Error('Expected eight physical seats');
    const label = player => player ? 'ABCDEFGH'[seats.indexOf(player)] || null : null;
    const B = seats.find(player => player !== A && player !== game.me && player !== game.zhu);
    if (!B || _status.currentPhase !== A) throw new Error('Expected a non-lord AI target in the current turn');
    // Character selection is random; retain whether each field survives rather
    // than persisting an unrelated randomly selected name in the golden file.
    const originalCharacter = [B.name || null, B.name1 || null, B.name2 || null];
    const card = item => [item.name, get.suit(item), get.number(item)];
    const ring = () => {
      const players = [A];
      while (players.at(-1).next !== A && players.length <= 8) players.push(players.at(-1).next);
      if (players.length > 8) throw new Error('Invalid live-seat ring');
      return players.map(label);
    };
    const roster = () => ({ players: game.players.map(label), dead: game.dead.map(label), ring: ring() });
    const state = () => ({
      character: [B.name || null, B.name1 || null, B.name2 || null]
        .map((name, index) => name === originalCharacter[index]),
      identity: B.identity,
      identityShown: !!B.identityShown, hp: B.hp, maxHp: B.maxHp, hujia: B.hujia,
      alive: B.isAlive(), out: B.isOut(), linked: B.isLinked(), turnedOver: B.isTurnedOver(),
      hand: B.getCards('h').map(card), equips: B.getCards('e').map(card),
      markCount: B.countMark('reference_revival_mark'), markShown: !!B.marks.reference_revival_mark,
      previous: label(B.previous), next: label(B.next), ...roster(), gameOver: !!_status.over,
    });
    const times = ['die', 'loseMaxHp', 'changeHp', 'revive', 'dying'].flatMap(name =>
      ['Before', 'Begin', '', 'End', 'After', 'Cancelled'].map(suffix => name + suffix));
    const events = [];
    let active = null, cancelRevive = false, activeRevive = null;
    function record(kind, timing, event) {
      events.push([active, kind, timing, event.name, event.getParent()?.name || null,
        event.num ?? null, event.hp ?? null, B.hp, B.maxHp, B.isAlive(),
        game.players.map(label), game.dead.map(label), !!event.finished, !!event._cancelled]);
    }
    const observer = 'reference_revival_observer', cancel = 'reference_revival_cancel';
    lib.skill.reference_revival_mark = { mark: true, intro: { content: 'mark' } };
    lib.skill[observer] = {
      trigger: { global: times }, firstDo: true, forced: true, silent: true, forceDie: true,
      filter: event => !!active && event.player === B,
      async content(event, trigger) { record('callback', event.triggername, trigger); },
    };
    lib.skill[cancel] = {
      trigger: { global: 'reviveBefore' }, forced: true, silent: true, forceDie: true,
      filter: event => cancelRevive && event === activeRevive,
      async content(event, trigger) { trigger.cancel(); },
    };
    for (const skill of ['reference_revival_mark', observer, cancel]) game.finishSkill(skill);
    A.addSkill(observer);
    A.addSkill(cancel);
    const proto = lib.element.GameEvent.prototype, originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (active && this.player === B && times.includes(timing)) record('request', timing, this);
      return originalTrigger.apply(this, arguments);
    };
    try {
      const existing = B.getCards('he');
      if (existing.length) await B.discard(existing);
      B.hp = B.maxHp = 4;
      B.hujia = 0;
      B.update();
      const hand = game.createCard2('sha', 'spade', 7);
      const equipment = game.createCard2('zhuge', 'club', 1);
      await B.gain(hand);
      await B.equip(equipment);
      B.addMark('reference_revival_mark', 2, false);
      await B.link(true);
      await B.turnOver(true);
      const setup = { seats: seats.map(label), target: label(B), actorCurrent: _status.currentPhase === A,
        targetNonLordAI: B !== game.zhu && B !== game.me, initial: state() };
      const stages = [];
      const capture = name => stages.push({ name, ...state() });

      active = 'death-full';
      await B.die().set('animate', false);
      capture('death-full');
      active = 'revive-two';
      const directReturn = B.revive(2, false);
      capture('revive-two');
      for (const [name, hp] of [['repeat-over-max', 9], ['repeat-zero', 0], ['repeat-negative', -2]]) {
        active = name;
        B.revive(hp, false);
        capture(name);
      }

      B.hp = B.maxHp = 1;
      B.update();
      active = 'death-negative-max';
      await B.loseMaxHp(2);
      capture('death-negative-max');
      active = 'revive-negative-max';
      B.revive(5, false);
      capture('revive-negative-max');

      B.hp = B.maxHp = 1;
      B.update();
      active = 'death-zero-max';
      await B.loseMaxHp(1);
      capture('death-zero-max');
      active = 'revive-event-cancel';
      cancelRevive = true;
      activeRevive = B.reviveEvent(2, false);
      await activeRevive;
      const cancelled = { hp: activeRevive.hp, finished: activeRevive.finished, cancelled: !!activeRevive._cancelled };
      capture('revive-event-cancel');
      cancelRevive = false;
      active = 'revive-event-zero';
      activeRevive = B.reviveEvent(0, false);
      await activeRevive;
      const restored = { hp: activeRevive.hp, finished: activeRevive.finished, cancelled: !!activeRevive._cancelled };
      capture('revive-event-zero');
      active = null;
      return { setup, directReturnedUndefined: directReturn === undefined, stages, cancelled, restored,
        eventColumns: ['trial', 'kind', 'timing', 'event', 'parent', 'num', 'eventHp', 'hp', 'maxHp',
          'alive', 'players', 'dead', 'finished', 'cancelled'], events,
        physicalCards: { handInDiscard: ui.discardPile.contains(hand), equipmentInDiscard: ui.discardPile.contains(equipment) } };
    } finally {
      proto.trigger = originalTrigger;
      A.removeSkill(observer);
      A.removeSkill(cancel);
    }
  };
}

export function checkReferenceRevival(result) {
  assert.deepEqual(result.setup.seats, [...'ABCDEFGH']);
  assert.equal(result.setup.actorCurrent, true);
  assert.equal(result.setup.targetNonLordAI, true);
  assert.equal(result.directReturnedUndefined, true);
  const { initial, target } = result.setup;
  assert.deepEqual([initial.hp, initial.maxHp, initial.alive, initial.identityShown, initial.linked, initial.turnedOver],
    [4, 4, true, false, true, true]);
  assert.deepEqual(initial.character, [true, true, true]);
  assert.deepEqual(initial.hand, [['sha', 'spade', 7]]);
  assert.deepEqual(initial.equips, [['zhuge', 'club', 1]]);
  assert.deepEqual([initial.markCount, initial.markShown], [2, true]);
  assert.deepEqual(initial.players, [...'ABCDEFGH']);
  assert.deepEqual(initial.ring, [...'ABCDEFGH']);
  assert.deepEqual(initial.dead, []);
  const stages = Object.fromEntries(result.stages.map(stage => [stage.name, stage]));
  const expected = {
    'death-full': [0, 4, false],
    'revive-two': [2, 4, true],
    'repeat-over-max': [4, 4, true],
    'repeat-zero': [1, 4, true],
    'repeat-negative': [-2, 4, true],
    'death-negative-max': [-1, -1, false],
    'revive-negative-max': [1, 1, true],
    'death-zero-max': [0, 0, false],
    'revive-event-cancel': [0, 0, false],
    'revive-event-zero': [1, 1, true],
  };
  assert.deepEqual(Object.keys(stages), Object.keys(expected));
  for (const [name, triple] of Object.entries(expected)) {
    const stage = stages[name];
    assert.deepEqual([stage.hp, stage.maxHp, stage.alive], triple, name);
    assert.deepEqual(stage.character, initial.character, name);
    assert.equal(stage.identity, initial.identity, name);
    assert.equal(stage.identityShown, true, name);
    assert.deepEqual([stage.out, stage.linked, stage.turnedOver, stage.gameOver], [false, false, false, false], name);
    assert.deepEqual([stage.hand, stage.equips, stage.markCount, stage.markShown], [[], [], 2, false], name);
    assert.deepEqual(stage.dead, stage.alive ? [] : [target], name);
    assert.deepEqual(stage.players, [...initial.players.filter(seat => seat !== target), ...(stage.alive ? [target] : [])], name);
    assert.deepEqual(stage.ring, initial.ring.filter(seat => stage.alive || seat !== target), name);
    assert.deepEqual([stage.previous, stage.next], [initial.previous, initial.next], name);
  }
  assert.deepEqual(result.physicalCards, { handInDiscard: true, equipmentInDiscard: true });
  assert.deepEqual(result.cancelled, { hp: 2, finished: true, cancelled: true });
  assert.deepEqual(result.restored, { hp: 1, finished: true, cancelled: false });

  const rows = result.events.map(row => Object.fromEntries(result.eventColumns.map((column, index) => [column, row[index]])));
  const requests = trial => rows.filter(row => row.trial === trial && row.kind === 'request');
  const expectedTimings = {
    'death-full': ['dieBefore', 'dieBegin', 'changeHpBefore', 'changeHpBegin', 'changeHp',
      'changeHpEnd', 'changeHpAfter', 'die', 'dieEnd', 'dieAfter'],
    'death-negative-max': ['loseMaxHpBefore', 'loseMaxHpBegin', 'dieBefore', 'dieBegin', 'changeHpBefore',
      'changeHpBegin', 'changeHp', 'changeHpEnd', 'changeHpAfter', 'die', 'dieEnd', 'dieAfter',
      'loseMaxHpEnd', 'loseMaxHpAfter'],
    'death-zero-max': ['loseMaxHpBefore', 'loseMaxHpBegin', 'dieBefore', 'dieBegin', 'die', 'dieEnd',
      'dieAfter', 'loseMaxHpEnd', 'loseMaxHpAfter'],
    'revive-event-cancel': ['reviveBefore', 'reviveCancelled'],
    'revive-event-zero': ['reviveBefore', 'reviveBegin', 'reviveEnd', 'reviveAfter'],
  };
  for (const [trial, timings] of Object.entries(expectedTimings)) {
    assert.deepEqual(requests(trial).map(row => row.timing), timings, trial);
    assert.deepEqual(rows.filter(row => row.trial === trial && row.kind === 'callback').map(row => row.timing), timings, trial);
  }
  for (const trial of ['revive-two', 'repeat-over-max', 'repeat-zero', 'repeat-negative', 'revive-negative-max']) {
    assert.deepEqual(requests(trial), [], trial);
  }
  const hpChange = trial => requests(trial).find(row => row.timing === 'changeHp');
  assert.deepEqual([hpChange('death-full').parent, hpChange('death-full').num, hpChange('death-full').hp], ['die', -4, 0]);
  assert.deepEqual([hpChange('death-negative-max').parent, hpChange('death-negative-max').num,
    hpChange('death-negative-max').hp, hpChange('death-negative-max').maxHp], ['die', 1, -1, -1]);
  assert.deepEqual(requests('revive-event-zero').map(row => [row.hp, row.maxHp, row.alive]),
    [[0, 0, false], [0, 0, false], [1, 1, true], [1, 1, true]]);
  const cancelled = rows.filter(row => row.trial === 'revive-event-cancel' && row.timing === 'reviveCancelled');
  assert.deepEqual(cancelled.map(row => [row.kind, row.finished, row.cancelled]),
    [['request', false, true], ['callback', true, true]]);
}
