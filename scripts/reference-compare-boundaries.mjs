import assert from 'node:assert/strict';

export function installCompareBoundaries() {
  window.__runCompareBoundaries = async ({ lib, game, ui, get, actor: A }) => {
    const C = game.me;
    if (A === C) {
      const hand = A.getCards('h');
      if (hand.length) await A.discard(hand);
      return;
    }
    const B = A.next === C ? A.previous : A.next;
    const trials = [];
    const events = [];
    let sample;
    let cards;
    let chosen;
    let root;
    const cardId = c => Object.keys(cards).find(k => cards[k] === c) || (c ? 'other' : null);
    const location = c => {
      const owner = get.owner(c);
      const pos = get.position(c, true);
      return owner === A ? 'A:' + pos : owner === B ? 'B:' + pos : pos || null;
    };
    const label = p => p === A ? 'A' : p === B ? 'B' : p ? 'other' : null;
    const marks = () => [A, B].map(player => player.getKnownCards(C).map(cardId).sort());
    const timings = new Set(['chooseCardAfter', 'loseAfter', 'compareCardShowBefore', 'showCardsAfter',
      'compare', 'compareFixing', 'chooseToCompareAfter', 'chooseToCompareCancelled', 'cardsDiscardAfter']);
    const record = (kind, timing, subject) => events.push([
      sample, kind, timing, subject.name, subject.getParent()?.name || null, label(subject.player),
      location(chosen), location(cards.bBid), ...marks(), (subject.cards || []).map(cardId),
    ]);
    const choose = (event, { state }) => {
      if (sample && state === 'begin' && event.getParent()?.name === 'chooseCardOL') {
        const bid = event.player === A ? chosen : cards.bBid;
        event.ai = card => card === bid ? 10 : 0;
      }
    };
    const probe = 'reference_compare_boundary';
    lib.skill[probe] = {
      trigger: { global: [...timings, 'loseBefore'] }, firstDo: true, forced: true, silent: true,
      async content(event, subject) {
        if (!sample) return;
        const timing = event.triggername;
        if (subject.getParent('chooseToCompare') !== root && subject !== root &&
            !(timing === 'cardsDiscardAfter' && subject.cards?.some(card => card === chosen || card === cards.bBid))) return;
        if (sample === 'cancel-second-loss' && timing === 'loseBefore' && subject.player === B) subject.cancel();
        if (timings.has(timing)) record('effect', timing, subject);
        if (sample === 'cancel-before-reveal' && timing === 'compareCardShowBefore') subject.cancel();
      },
    };
    game.finishSkill(probe);
    A.addSkill(probe);
    game.globalEventHandlers.pushHandler('chooseCard', choose);
    const original = lib.element.GameEvent.prototype.trigger;
    lib.element.GameEvent.prototype.trigger = function (timing) {
      if (sample && timings.has(timing) && (this.getParent('chooseToCompare') === root || this === root ||
          (timing === 'cardsDiscardAfter' && this.cards?.some(card => card === chosen || card === cards.bBid)))) {
        record('request', timing, this);
      }
      return original.apply(this, arguments);
    };
    try {
      for (const name of ['bid-hidden', 'bid-known', 'cancel-before-reveal', 'cancel-second-loss']) {
        sample = null;
        for (const player of [A, B]) {
          const held = player.getCards('he');
          if (held.length) await player.discard(held);
        }
        cards = { aKnown: game.createCard2('tao', 'heart', 11), aHidden: game.createCard2('sha', 'spade', 10),
          bBid: game.createCard2('shan', 'diamond', 5), bOther: game.createCard2('tao', 'heart', 2) };
        await A.gain(cards.aKnown, 'gain2');
        ui.cardPile.insertBefore(cards.aHidden, ui.cardPile.firstChild);
        await A.draw(1);
        ui.cardPile.insertBefore(cards.bOther, ui.cardPile.firstChild);
        ui.cardPile.insertBefore(cards.bBid, ui.cardPile.firstChild);
        await B.draw(2);
        chosen = name === 'bid-known' ? cards.aKnown : cards.aHidden;
        const initialKnowledge = marks();
        sample = name;
        root = A.chooseToCompare(B);
        await root;
        const result = root.result;
        trials.push({ sample, initialKnowledge, chosen: cardId(chosen), finalZones: [location(chosen), location(cards.bBid)],
          finalKnowledge: marks(), cancelled: !!root._cancelled, hasResult: typeof result?.bool === 'boolean',
          won: result?.bool ?? null, points: result?.num1 != null ? [result.num1, result.num2] : null });
      }
    } finally {
      lib.element.GameEvent.prototype.trigger = original;
      game.globalEventHandlers.removeHandler('chooseCard', game.globalEventHandlers.getDefaultHandlerType('chooseCard'), choose);
      A.removeSkill(probe);
    }
    return { players: game.players.length, trials,
      eventColumns: ['sample', 'kind', 'timing', 'event', 'parent', 'seat', 'bidA', 'bidB', 'knownA', 'knownB', 'eventCards'], events };
  };
}

export function checkCompareBoundaries({ trials, events }) {
  assert.equal(trials.length, 4);
  for (const trial of trials) assert.deepEqual(trial.initialKnowledge, [['aKnown'], []]);
  for (const sample of ['bid-hidden', 'bid-known']) {
    const loss = events.find(r => r[0] === sample && r[1] === 'effect' && r[2] === 'loseAfter' && r[5] === 'A');
    assert.deepEqual(loss[8], [], 'a hidden payment obscures the identity of the remaining hand');
  }
  assert.deepEqual(trials[2].finalZones, ['d', 'd']);
  assert.equal(trials[2].cancelled, true);
  assert.equal(trials[2].hasResult, false);
  assert.ok(!events.some(row => row[0] === 'cancel-before-reveal' && ['showCardsAfter', 'compare', 'compareFixing'].includes(row[2])));
  assert.deepEqual(trials[3].finalZones, ['d', 'B:h']);
  assert.equal(trials[3].hasResult, true);
  assert.deepEqual(trials[3].points, [10, 5]);
  assert.equal(trials[3].won, true);
}
