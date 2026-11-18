import assert from 'node:assert/strict';

export function installRecastBatchBoundaries() {
  window.__runRecastBatchBoundaries = async ({ lib, game, ui, get, actor: A, target: B }) => {
    const C = game.me;
    if (A === C) {
      if (C.countCards('h')) await C.discard(C.getCards('h'));
      return;
    }
    const materialIds = ['shown', 'hidden', 'equipped'];
    const replacementIds = ['draw-1', 'draw-2', 'draw-3'];
    const labels = [...materialIds, 'control', ...replacementIds];
    const sampleNames = ['cancel-loss-wrapper', 'intercept-all'];
    const events = [];
    const trials = [];
    let trial;
    const seat = player => player === A ? 'A' : player === B ? 'B' : player === C ? 'C' : player ? 'other' : null;
    const parent = event => event.getParent()?.name || null;
    const ids = (cards = []) => cards.map(card => trial.byCard.get(card)).filter(Boolean);
    const zone = card => {
      const owner = get.owner(card);
      const position = get.position(card, true);
      return owner ? `${seat(owner)}:${position}` : position || null;
    };
    const zones = () => labels.map(label => zone(trial.byId[label]));
    const spectatorHands = () => [A, B].map(player => ids(player.getKnownCards(C)));
    const underRoot = event => !!trial?.root && event.getParent('recast', true, true) === trial.root;
    const summary = event => !event ? null : {
      event: event.name, parent: parent(event), cards: ids(event.cards),
      finished: event.finished, cancelled: !!event._cancelled,
      ...(event.stockcards ? { stockcards: ids(event.stockcards) } : {}),
      ...(event.name === 'draw' ? { amount: event.num, drawn: ids(event.result?.cards) } : {}),
    };
    function capture(kind, timing, event, detail) {
      events.push([trial.name, kind, timing, event.name, parent(event), seat(event.player), seat(event.source),
        zones(), spectatorHands(), detail]);
    }
    const observe = (kind, timing, event, observer) => capture(kind, timing, event, {
      ...summary(event),
      ...(observer ? { callbackParent: parent(observer) } : {}),
      ...(typeof event.visible === 'boolean' ? { visible: event.visible } : {}),
    });
    const timings = new Set(['recastBefore', 'recastingLose', 'recastingLost', 'recast',
      'recastingGain', 'recastingGained', 'recastAfter', 'recastCancelled',
      'loseToDiscardpileBefore', 'loseToDiscardpileBegin', 'loseToDiscardpileCancelled',
      'loseToDiscardpile', 'loseToDiscardpileEnd', 'loseToDiscardpileAfter',
      'loseBefore', 'loseBegin', 'loseEnd', 'loseAfter', 'loseCancelled', 'gainAfter', 'drawBefore', 'drawAfter']);
    const callbackTimings = ['loseToDiscardpileBegin', 'loseToDiscardpileCancelled', 'loseToDiscardpile',
      'loseToDiscardpileEnd', 'loseToDiscardpileAfter', 'loseBefore', 'loseEnd', 'loseAfter', 'loseCancelled',
      'recastingLost', 'recast', 'recastingGained', 'recastAfter'];
    const observer = 'reference_recast_batch_boundary_observer';
    lib.skill[observer] = {
      trigger: { global: callbackTimings }, firstDo: true, forced: true, silent: true,
      filter: event => underRoot(event),
      async content(event, subject) {
        const timing = event.triggername;
        observe('callback', timing, subject, event);
        if (trial.name === 'cancel-loss-wrapper' && timing === 'loseToDiscardpileBegin') {
          subject.cancel();
        } else if (trial.name === 'intercept-all' && timing === 'loseBefore' && parent(subject) === 'loseToDiscardpile') {
          await A.give(materialIds.map(id => trial.byId[id]), B);
          observe('probe', 'intercept-returned', subject);
        }
      },
    };
    game.finishSkill(observer);
    A.addSkill(observer);
    const proto = lib.element.GameEvent.prototype;
    const original = proto.trigger;
    proto.trigger = function (timing) {
      if (trial && timing === 'recastBefore' && this.player === A) {
        trial.root = this;
        for (const name of ['recastingLose', 'recastingGain']) {
          const factory = this[name];
          this[name] = function (player, cards) {
            const child = factory.call(this, player, cards);
            capture('factory', name, this, { cards: ids(cards), sameArray: cards === this.cards, child: summary(child) });
            return child;
          };
        }
      }
      if (timings.has(timing) && underRoot(this)) observe('request', timing, this);
      return original.apply(this, arguments);
    };

    try {
      for (const name of sampleNames) {
        trial = null;
        for (const player of [A, B]) {
          const held = player.getCards('he');
          if (held.length) await player.discard(held);
        }
        const byId = {
          shown: game.createCard2('tiesuo', 'club', 10), hidden: game.createCard2('tiesuo', 'club', 11),
          equipped: game.createCard2('qinggang', 'spade', 6), control: game.createCard2('sha', 'spade', 7),
          'draw-1': game.createCard2('tao', 'heart', 2), 'draw-2': game.createCard2('shan', 'diamond', 3),
          'draw-3': game.createCard2('sha', 'spade', 4),
        };
        await A.equip(byId.equipped);
        ui.cardPile.prepend(byId.hidden);
        await A.draw(1);
        await A.gain([byId.shown, byId.control], 'gain2');
        ui.cardPile.prepend(...replacementIds.map(id => byId[id]));
        trial = { name, byId, byCard: new Map(Object.entries(byId).map(([id, card]) => [card, id])) };
        const before = { zones: zones(), spectatorHands: spectatorHands() };
        window.__referenceStages.push(`${name}: materials ready`);
        const root = A.recast(materialIds.map(id => byId[id]));
        await root;
        const paths = [A.getCards('he'), B.getCards('he'), [...ui.cardPile.children],
          [...ui.discardPile.children], [...ui.special.children], [...ui.ordering.children]];
        trials.push({
          name, before, after: { zones: zones(), spectatorHands: spectatorHands(),
            recipientHands: [A, B].map(player => ids(player.getKnownCards(B))),
            occurrences: labels.map(id => paths.flat().filter(card => card === byId[id]).length),
            losses: root.recastingLosingEvents.map(summary), innerLoss: summary(root.recastingLosingEvents[0].done),
            gains: root.recastingGainingEvents.map(summary), root: summary(root),
            recordedLosses: A.getHistory('lose').filter(underRoot).map(summary),
          },
        });
      }
    } finally {
      proto.trigger = original;
      A.removeSkill(observer);
    }
    return { players: game.players.length, spectator: seat(C), labels,
      eventColumns: ['sample', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'zones', 'spectatorHands', 'detail'],
      trials, events };
  };
}

export function checkRecastBatchBoundaries(result) {
  assert.equal(result.players, 8);
  assert.equal(result.spectator, 'C');
  assert.deepEqual(result.trials.map(trial => trial.name), ['cancel-loss-wrapper', 'intercept-all']);
  const rows = result.events.map(row => Object.fromEntries(result.eventColumns.map((column, index) => [column, row[index]])));
  const materials = ['shown', 'hidden', 'equipped'];
  const replacements = ['draw-1', 'draw-2', 'draw-3'];
  const originalZones = ['A:h', 'A:h', 'A:e', 'A:h', 'c', 'c', 'c'];
  const originalKnowledge = [['shown', 'control'], []];
  const ordered = (...sequence) => sequence.every((row, index) => index === 0 || rows.indexOf(row) > rows.indexOf(sequence[index - 1]));
  for (const { name, before, after } of result.trials) {
    const events = rows.filter(row => row.sample === name);
    const by = (kind, timing, parent) => events.filter(row =>
      row.kind === kind && row.timing === timing && (parent === undefined || row.parent === parent));
    const one = (kind, timing, parent) => {
      const matches = by(kind, timing, parent);
      assert.equal(matches.length, 1, `Expected one ${kind}/${timing} in ${name}`);
      return matches[0];
    };
    const wrapperCancelled = name === 'cancel-loss-wrapper';
    const loss = { event: 'loseToDiscardpile', parent: 'recast', cards: materials, finished: true, cancelled: wrapperCancelled };
    const draw = { event: 'draw', parent: 'recast', cards: [], finished: true, cancelled: false, amount: 3, drawn: replacements };
    const emptyLoss = {
      event: 'lose', parent: 'loseToDiscardpile', cards: [], finished: true, cancelled: false, stockcards: materials,
    };
    const held = wrapperCancelled ? originalZones.slice(0, 3) : ['B:h', 'B:h', 'B:h'];
    assert.deepEqual(before, { zones: originalZones, spectatorHands: originalKnowledge });
    assert.deepEqual(after.zones, [...held, 'A:h', 'A:h', 'A:h', 'A:h']);
    assert.deepEqual(after.occurrences, [1, 1, 1, 1, 1, 1, 1]);
    assert.deepEqual(after.losses, [loss]);
    assert.deepEqual(after.gains, [draw]);
    assert.deepEqual(after.innerLoss, wrapperCancelled ? null : emptyLoss);
    assert.deepEqual(after.root, {
      event: 'recast', parent: 'reference_fixture_use', cards: materials, finished: true, cancelled: false,
    });
    assert.deepEqual(after.spectatorHands, wrapperCancelled ? originalKnowledge : [[], ['equipped']]);
    assert.deepEqual(after.recipientHands, wrapperCancelled ? originalKnowledge : [[], materials]);
    assert.deepEqual(after.recordedLosses, wrapperCancelled ? [] : [
      { ...emptyLoss, parent: 'gain', cards: materials }, emptyLoss,
    ]);

    const paid = one('callback', 'recastingLost');
    const lossFactory = one('factory', 'recastingLose');
    const gainFactory = one('factory', 'recastingGain');
    const drawAfter = one('request', 'drawAfter');
    assert.deepEqual(lossFactory.detail, {
      cards: materials, sameArray: true, child: { ...loss, finished: false, cancelled: false },
    });
    assert.deepEqual(gainFactory.detail, {
      cards: materials, sameArray: true, child: { ...draw, finished: false, drawn: [] },
    });
    assert.deepEqual([drawAfter.player, drawAfter.source, drawAfter.detail], ['A', 'A', draw]);
    assert.deepEqual(paid.zones, [...held, 'A:h', 'c', 'c', 'c']);
    assert.ok(ordered(lossFactory, one('request', 'recastingLost'), paid, gainFactory, drawAfter, one('callback', 'recastAfter')));
    assert.equal(by('request', 'recastCancelled').length, 0);

    const wrapperRequests = events.filter(row => row.kind === 'request' && row.event === 'loseToDiscardpile');
    assert.deepEqual(wrapperRequests.map(row => row.timing), ['loseToDiscardpileBefore', 'loseToDiscardpileBegin',
      ...(wrapperCancelled ? ['loseToDiscardpileCancelled'] : ['loseToDiscardpile', 'loseToDiscardpileEnd', 'loseToDiscardpileAfter'])]);
    assert.ok(wrapperRequests.every(row => row.parent === 'recast'));
    if (wrapperCancelled) {
      const request = one('request', 'loseToDiscardpileCancelled');
      const callback = one('callback', 'loseToDiscardpileCancelled');
      assert.deepEqual(request.detail, { ...loss, finished: false });
      assert.deepEqual(callback.detail, { ...loss, callbackParent: 'trigger' });
      assert.ok(ordered(request, callback, paid));
      assert.ok(events.every(row => row.event !== 'lose'));
    } else {
      const realLoss = one('request', 'loseAfter', 'gain');
      const gain = one('request', 'gainAfter', 'reference_recast_batch_boundary_observer');
      const returned = one('probe', 'intercept-returned');
      const emptyEnd = one('request', 'loseEnd', 'loseToDiscardpile');
      const emptyAfter = one('request', 'loseAfter', 'loseToDiscardpile');
      assert.deepEqual(realLoss.zones.slice(0, 3), ['s', 's', 's']);
      assert.deepEqual([gain.player, gain.source, gain.detail.cards, gain.zones.slice(0, 3)], ['B', 'A', materials, held]);
      assert.ok(ordered(realLoss, gain, returned, emptyEnd, emptyAfter, one('callback', 'loseToDiscardpileAfter'), paid));
      for (const timing of ['loseEnd', 'loseAfter']) {
        assert.deepEqual(one('request', timing, 'loseToDiscardpile').detail, { ...emptyLoss, visible: true });
        assert.deepEqual(one('callback', timing, 'loseToDiscardpile').detail, { ...emptyLoss, visible: true, callbackParent: 'trigger' });
      }
      assert.equal(by('request', 'loseCancelled').length, 0);
    }
  }
}
