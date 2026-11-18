import assert from 'node:assert/strict';

export function installBatchGiveDeath() {
  window.__runBatchGiveDeath = async ({ lib, game, ui, get, _status, actor: A }) => {
    const B = game.players.find(player => player !== A && player !== game.me && player !== game.zhu);
    const C = game.players.find(player => player !== A && player !== B);
    if (!A || !B || !C || new Set([A, B, C]).size !== 3 || _status.currentPhase !== A ||
        !A.isAlive() || !B.isAlive() || !C.isAlive()) {
      throw new Error('Expected a live actor, a non-lord recipient, and a distinct live observer');
    }
    const setup = {
      distinct: new Set([A, B, C]).size === 3,
      actorIsCurrent: _status.currentPhase === A,
      recipientIsLord: B === game.zhu,
      recipientIsHuman: B === game.me,
      alive: [A, B, C].map(player => player.isAlive()),
    };
    const hand = game.createCard2('tiesuo', 'club', 11);
    const equipment = game.createCard2('qinggang', 'spade', 6);
    await A.equip(equipment);
    ui.cardPile.prepend(hand);
    await A.draw(1);

    const seat = player => player === A ? 'A' : player === B ? 'B' : player === C ? 'C' : player ? 'other' : null;
    const cards = [hand, equipment];
    const ids = (items = []) => items.map(card => card === hand ? 'h' : card === equipment ? 'e' : '?');
    const snapshot = () => cards.map(card => ({
      zone: get.position(card, true) || null,
      owner: seat(get.owner(card)),
      special: ui.special.contains(card),
      known: [A, B, C].map(player => card.isKnownBy(player)),
      occurrences: [ui.special, ui.discardPile, ui.cardPile, ui.ordering,
        ...[...game.players, ...game.dead].flatMap(player => [player.node.handcards1, player.node.handcards2,
          player.node.equips, player.node.judges, player.node.expansions])]
        .filter(zone => zone?.contains(card)).length,
    }));
    const events = [];
    let gift;
    const capture = (kind, timing, subject) => events.push({
      kind, timing, event: subject.name, parent: subject.getParent()?.name || null,
      player: seat(subject.player), alive: B.isAlive(), cards: ids(subject.cards), state: snapshot(),
      cancelled: !!subject._cancelled, finished: subject.finished,
    });
    const timings = ['gainBefore', 'gainBegin', 'gainEnd', 'gainAfter', 'gainCancelled',
      'loseBefore', 'loseBegin', 'loseEnd', 'loseAfter', 'loseCancelled', 'dieBefore', 'die', 'dieAfter'];
    const relevant = subject => subject === gift ||
      (subject.name === 'lose' && subject.getParent() === gift) ||
      (subject.name === 'die' && subject.player === B);
    const proto = lib.element.GameEvent.prototype;
    const originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (timings.includes(timing) && relevant(this)) capture('request', timing, this);
      return originalTrigger.apply(this, arguments);
    };
    const observer = 'reference_batch_give_death';
    lib.skill[observer] = {
      trigger: { global: timings }, forced: true, silent: true, firstDo: true, forceDie: true,
      filter: subject => relevant(subject),
      async content(event, subject) {
        capture('callback', event.triggername, subject);
        if (event.triggername === 'loseAfter' && subject.getParent() === gift) {
          await B.die().set('animate', false);
          capture('callback', 'returned-from-die', subject);
        }
      },
    };
    game.finishSkill(observer);
    A.addSkill(observer);
    try {
      const before = snapshot();
      gift = A.give(cards, B);
      await gift;
      return {
        setup, before, after: snapshot(),
        gain: { player: seat(gift.player), finished: gift.finished, cancelled: !!gift._cancelled,
          forceDie: !!gift.forceDie, cards: ids(gift.cards) },
        alive: [A, B, C].map(player => player.isAlive()),
        slots: game.players.length + game.dead.length,
        remaining: game.players.length,
        gameOver: !!_status.over,
        events,
      };
    } finally {
      proto.trigger = originalTrigger;
      A.removeSkill(observer);
    }
  };
}

export function checkBatchGiveDeath(result) {
  assert.deepEqual(result.setup, {
    distinct: true, actorIsCurrent: true, recipientIsLord: false, recipientIsHuman: false,
    alive: [true, true, true],
  });
  assert.equal(result.slots, 8);
  assert.equal(result.remaining, 7);
  assert.equal(result.gameOver, false);
  assert.deepEqual(result.alive, [true, false, true]);
  const held = [
    { zone: 'h', owner: 'A', special: false, known: [true, false, false], occurrences: 1 },
    { zone: 'e', owner: 'A', special: false, known: [true, true, true], occurrences: 1 },
  ];
  const transit = [
    { zone: 's', owner: null, special: true, known: [false, false, false], occurrences: 1 },
    { zone: 's', owner: null, special: true, known: [true, true, true], occurrences: 1 },
  ];
  assert.deepEqual(result.before, held);
  assert.deepEqual(result.after, transit);
  assert.deepEqual(result.gain, {
    player: 'B', finished: true, cancelled: false, forceDie: false, cards: ['h', 'e'],
  });
  const rows = (kind, timing) => result.events.filter(row => row.kind === kind && row.timing === timing);
  const index = (kind, timing) => result.events.findIndex(row => row.kind === kind && row.timing === timing);
  for (const kind of ['request', 'callback']) {
    for (const timing of ['gainBefore', 'gainBegin', 'gainEnd', 'gainAfter']) {
      const matching = rows(kind, timing);
      assert.equal(matching.length, 1, `${kind} ${timing}`);
      assert.equal(matching[0].event, 'gain');
      assert.equal(matching[0].player, 'B');
      assert.equal(matching[0].cancelled, false);
      assert.deepEqual(matching[0].cards, ['h', 'e']);
      assert.equal(matching[0].finished, timing === 'gainEnd' || timing === 'gainAfter');
      assert.equal(matching[0].alive, timing === 'gainBefore' || timing === 'gainBegin');
    }
  }
  assert.equal(rows('request', 'gainCancelled').length, 0);
  assert.equal(rows('request', 'loseCancelled').length, 0);
  const entered = index('request', 'loseEnd');
  const deathBefore = index('request', 'dieBefore');
  const deathAfter = index('request', 'dieAfter');
  const returned = index('callback', 'returned-from-die');
  const gainEnd = index('request', 'gainEnd');
  const gainAfter = index('request', 'gainAfter');
  assert.ok(entered >= 0 && entered < deathBefore && deathBefore < deathAfter &&
    deathAfter < returned && returned < gainEnd && gainEnd < gainAfter);
  assert.equal(result.events[deathBefore].alive, true);
  assert.equal(result.events[deathAfter].alive, false);
  for (const [i, row] of result.events.entries()) {
    assert.deepEqual(row.state, i < entered ? held : transit);
    assert.ok(row.state.every(card => card.occurrences === 1), 'each physical card occupies exactly one zone');
  }
}
