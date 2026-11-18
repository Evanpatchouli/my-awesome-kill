import assert from 'node:assert/strict';

export function installRecastBatch() {
  window.__runRecastBatch = async ({ lib, game, ui, get, actor: A, target: B }) => {
    const C = game.me;
    if (A === C) {
      const hand = C.getCards('h');
      if (hand.length) await C.discard(hand);
      return;
    }
    if (B === C) throw new Error('Expected the two affected seats to use offline AI');
    const pack = (await import('/character/clan/skill.js')).default;
    if (!lib.skill.clanmingdian) {
      lib.skill.clanmingdian = pack.clanmingdian;
      game.finishSkill('clanmingdian');
    }
    lib.config.game_speed = 'vvfast';

    const samples = ['default', 'intercept-one', 'cancel-loss', 'cancel-draw', 'public-gain', 'clan-choice'];
    const eventColumns = ['sample', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'target', 'zones', 'detail'];
    const labels = ['shown', 'hidden', 'equipped', 'control', 'draw-1', 'draw-2', 'draw-3'];
    const events = [];
    const trials = [];
    const seat = player => player === A ? 'A' : player === B ? 'B' : player === C ? 'C' : player ? 'other' : null;
    const parent = event => event.getParent()?.name || null;
    let trial;
    const ids = (cards = []) => cards.map(card => trial.cards.get(card)).filter(Boolean);
    const zone = card => {
      const owner = get.owner(card);
      const position = get.position(card, true);
      return owner ? `${seat(owner)}:${position}` : position || null;
    };
    const zones = () => labels.map(name => trial.byId[name] ? zone(trial.byId[name]) : null);
    const playerHand = player => ids(player.getCards('h'));
    const knownHands = viewer => [A, B].map(owner => ids(owner.getKnownCards(viewer)));
    const discardOrder = () => ids([...ui.discardPile.children]);
    const pileOrder = () => ids([...ui.cardPile.children]);
    const describe = event => ({
      event: event.name, parent: parent(event), cards: ids(event.cards),
      num: event.name === 'draw' ? event.num : null, cancelled: !!event._cancelled, finished: event.finished,
    });
    const losingEvents = root => root.recastingLosingEvents.map(describe);
    const gainingEvents = root => root.recastingGainingEvents.map(describe);
    const underRoot = event => !!trial?.root && event.getParent('recast', true, true) === trial.root;

    function capture(kind, timing, event, detail = null) {
      events.push([trial.sample, kind, timing, event.name, parent(event), seat(event.player), seat(event.source),
        seat(event.target), zones(), detail]);
    }

    function eventState(event, timing, observer) {
      const detail = {};
      if (observer) detail.callbackParent = parent(observer);
      if (event.cards) detail.cards = ids(event.cards);
      if (typeof event.visible === 'boolean') detail.visible = event.visible;
      if (event.name === 'recast') {
        detail.finished = event.finished;
        detail.cancelled = !!event._cancelled;
        detail.losingEvents = losingEvents(event);
        detail.gainingEvents = gainingEvents(event);
      }
      if (event.name === 'lose') {
        if (event.stockcards) detail.stockCards = ids(event.stockcards);
        if (event.hs || event.es) detail.lostFrom = { h: ids(event.hs), e: ids(event.es) };
      }
      if (event.name === 'draw') {
        detail.num = event.num;
        detail.drawn = ids(event.result?.cards);
      }
      if (['loseAfter', 'gainAfter', 'recastingLost', 'recastingGained'].includes(timing)) {
        detail.hands = [A, B].map(playerHand);
        detail.spectatorKnownHands = knownHands(C);
        detail.discardOrder = discardOrder();
      }
      if (event.name === 'chooseCard') {
        const range = get.select(event.selectCard);
        detail.position = event.position;
        detail.range = [range[0], Number.isFinite(range[1]) ? range[1] : 'unbounded'];
        detail.offered = ids(A.getCards(event.position));
        detail.eligible = ids(A.getCards(event.position).filter(card => event.filterCard(card, A, event)));
        detail.selected = ids(event.result?.cards);
        detail.accepted = event.result?.bool ?? null;
        detail.spectatorKnownHands = knownHands(C);
        detail.discardOrder = discardOrder();
      }
      return detail;
    }

    const timings = ['recastBefore', 'recastBegin', 'recastingLose', 'recastingLost', 'recast',
      'recastingGain', 'recastingGained', 'recastEnd', 'recastAfter',
      'loseToDiscardpileBefore', 'loseToDiscardpileAfter', 'loseBefore', 'loseAfter', 'loseCancelled',
      'drawBefore', 'drawAfter', 'drawCancelled', 'gainBefore', 'gainAfter', 'chooseCardBegin', 'chooseCardAfter'];
    const observed = ['recastingLose', 'recastingLost', 'recast', 'recastingGain', 'recastingGained', 'recastAfter',
      'loseBefore', 'loseAfter', 'loseCancelled', 'drawBefore', 'drawAfter', 'drawCancelled', 'gainAfter', 'chooseCardAfter'];
    function watched(event, timing) {
      if (!trial || !timings.includes(timing)) return false;
      return underRoot(event) || ['chooseCardBegin', 'chooseCardAfter'].includes(timing) &&
        parent(event) === 'clanmingdian_cost';
    }

    const observer = 'reference_recast_batch_observer';
    lib.skill[observer] = {
      trigger: { global: observed }, firstDo: true, forced: true, silent: true,
      filter: (event, player, timing) => watched(event, timing),
      async content(event, subject) {
        const timing = event.triggername;
        capture('callback', timing, subject, eventState(subject, timing, event));
        if (timing === 'loseBefore' && parent(subject) === 'loseToDiscardpile') {
          if (trial.sample === 'intercept-one') {
            await A.give(trial.byId.hidden, B);
            capture('probe', 'intercept-returned', subject, { hands: [A, B].map(playerHand), spectatorKnownHands: knownHands(C) });
          }
          if (trial.sample === 'cancel-loss') subject.cancel();
        }
        if (trial.sample === 'cancel-draw' && timing === 'drawBefore') subject.cancel();
      },
    };
    game.finishSkill(observer);
    A.addSkill(observer);

    game.globalEventHandlers.pushHandler('chooseCard', (event, { state }) => {
      if (trial && state === 'begin' && parent(event) === 'clanmingdian_cost') {
        event.ai = card => [trial.byId.shown, trial.byId.hidden].includes(card) ? 10 : -10;
      }
    });

    function observeFactories(root) {
      for (const factory of ['recastingLose', 'recastingGain']) {
        const original = root[factory];
        root[factory] = function (player, cards) {
          const child = original.call(this, player, cards);
          capture('factory', factory, this, { player: seat(player), cards: ids(cards), sameArray: cards === this.cards,
            child: describe(child) });
          return child;
        };
      }
    }

    const proto = lib.element.GameEvent.prototype;
    const originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (trial && timing === 'recastBefore' && this.player === A) {
        if (trial.root) throw new Error('Expected one recast root per trial');
        trial.root = this;
        observeFactories(this);
      }
      if (watched(this, timing)) capture('request', timing, this, eventState(this, timing));
      return originalTrigger.apply(this, arguments);
    };
    const originalLog = game.log;
    game.log = function (player, message, cards) {
      const result = originalLog.apply(this, arguments);
      if (trial?.root && player === A && message === '重铸了') {
        capture('display', 'recast-log', trial.root, {
          cards: ids(cards), inSharedLog: !!ui.sidebar.firstElementChild?.textContent.includes(get.translation(cards)),
          spectatorKnownHands: knownHands(C),
        });
      }
      return result;
    };

    const make = (name, suit, number) => game.createCard2(name, suit, number);
    try {
      for (const sample of samples) {
        trial = null;
        for (const player of [A, B]) {
          const hand = player.getCards('he');
          if (hand.length) await player.discard(hand);
          player.maxHp = 4;
          player.hp = 4;
          player.hujia = 0;
          player.update();
        }
        const clan = sample === 'clan-choice';
        const shown = make(clan ? 'shan' : 'tiesuo', 'club', 10);
        const hidden = make(clan ? 'sha' : 'tiesuo', 'club', 11);
        const control = make(clan ? 'tiesuo' : 'sha', 'spade', 7);
        const equipped = clan ? null : make('qinggang', 'spade', 6);
        if (equipped) await A.equip(equipped);
        ui.cardPile.prepend(hidden);
        await A.draw(1);
        await A.gain([shown, control], 'gain2');
        const replacements = [make('tao', 'heart', 2), make('shan', 'diamond', 3), make('sha', 'spade', 4)];
        ui.cardPile.prepend(...replacements);
        const byId = { shown, hidden, equipped, control,
          'draw-1': replacements[0], 'draw-2': replacements[1], 'draw-3': replacements[2] };
        trial = { sample, byId, cards: new Map(Object.entries(byId).filter(([, card]) => card).map(([name, card]) => [card, name])) };
        const before = { zones: zones(), hands: [A, B].map(playerHand), spectatorKnownHands: knownHands(C),
          pileOrder: pileOrder(), useCounts: { ...A.getStat('card') } };
        const faces = Object.fromEntries(Object.entries(byId).filter(([, card]) => card).map(([label, card]) => [
          label, [card.name, get.suit(card), get.number(card)],
        ]));
        const materials = clan ? [shown, hidden] : [shown, hidden, equipped];
        const eligibility = materials.map(card => [A.canRecast(card, null, true), A.canRecast(card)]);
        window.__referenceStages.push(`${sample}: materials ready`);
        if (clan) {
          A.addSkill('clanmingdian');
          await A.damage(B);
          A.removeSkill('clanmingdian');
        } else if (sample === 'public-gain') {
          await A.recast(materials, undefined, player => player.gain(replacements, 'gain2'));
        } else {
          await A.recast(materials);
        }
        if (!trial.root) throw new Error(`No recast started for ${sample}`);
        const root = trial.root;
        const holderZones = [A, B].flatMap(player => ['h', 'e'].map(area => player.getCards(area)));
        const locations = [...holderZones, [...ui.cardPile.children], [...ui.discardPile.children], [...ui.ordering.children]];
        const occurrences = labels.map(name => byId[name] ? locations.flat().filter(card => card === byId[name]).length : null);
        trials.push({ sample, faces, eligibility, before,
          after: { zones: zones(), hands: [A, B].map(playerHand), spectatorKnownHands: knownHands(C),
            recipientKnownHands: knownHands(B), pileOrder: pileOrder(),
            discardOrder: discardOrder(), useCounts: { ...A.getStat('card') }, occurrences,
            root: describe(root), losingEvents: losingEvents(root), gainingEvents: gainingEvents(root),
            innerLoss: describe(root.recastingLosingEvents[0].done) },
        });
      }
      return { players: game.players.length, spectator: seat(C), labels, eventColumns, trials, events };
    } finally {
      proto.trigger = originalTrigger;
      game.log = originalLog;
      A.removeSkill(observer);
    }
  };
}

export function checkRecastBatch(result) {
  assert.equal(result.players, 8);
  assert.equal(result.spectator, 'C');
  assert.deepEqual(result.trials.map(trial => trial.sample), [
    'default', 'intercept-one', 'cancel-loss', 'cancel-draw', 'public-gain', 'clan-choice',
  ]);
  const events = result.events.map(row => Object.fromEntries(result.eventColumns.map((column, index) => [column, row[index]])));
  const getRows = (sample, kind, timing) => events.filter(row => row.sample === sample && row.kind === kind && row.timing === timing);
  const one = (sample, kind, timing) => {
    const rows = getRows(sample, kind, timing);
    assert.equal(rows.length, 1, `Expected one ${kind}/${timing} in ${sample}`);
    return rows[0];
  };
  const ordered = (...rows) => rows.every((row, index) => index === 0 || events.indexOf(row) > events.indexOf(rows[index - 1]));
  const replacements = ['draw-1', 'draw-2', 'draw-3'];
  for (const { sample, eligibility, before, after } of result.trials) {
    const clan = sample === 'clan-choice';
    const count = clan ? 2 : 3;
    const materials = clan ? ['shown', 'hidden'] : ['shown', 'hidden', 'equipped'];
    const cancelDraw = sample === 'cancel-draw';
    const cancelLoss = sample === 'cancel-loss';
    const intercepted = sample === 'intercept-one';
    const publicGain = sample === 'public-gain';
    const gainEvent = publicGain ? 'gain' : 'draw';
    const discarded = cancelLoss ? [] : intercepted ? ['shown', 'equipped'] : materials;
    const stayed = clan ? ['draw-3'] : cancelDraw ? replacements : [];
    const drawn = replacements.filter(card => !stayed.includes(card));
    assert.deepEqual(eligibility, clan ? [[false, true], [false, true]] : [[true, true], [true, true], [false, true]]);
    assert.deepEqual(after.occurrences, [1, 1, clan ? null : 1, 1, 1, 1, 1]);
    assert.deepEqual(after.useCounts, before.useCounts);
    assert.deepEqual(before.pileOrder, replacements);
    assert.deepEqual(after.discardOrder, discarded);
    assert.deepEqual(after.pileOrder, stayed);
    assert.deepEqual(after.zones, [
      cancelLoss ? 'A:h' : 'd', cancelLoss ? 'A:h' : intercepted ? 'B:h' : 'd', clan ? null : cancelLoss ? 'A:e' : 'd',
      'A:h', ...replacements.map(card => stayed.includes(card) ? 'c' : 'A:h'),
    ]);
    assert.deepEqual(after.hands, [
      [...drawn, ...(cancelLoss ? ['shown', 'control', 'hidden'] : ['control'])], intercepted ? ['hidden'] : [],
    ]);
    assert.deepEqual(before.spectatorKnownHands, [['shown', 'control'], []]);
    assert.deepEqual(after.spectatorKnownHands, [
      intercepted ? [] : cancelLoss ? ['shown', 'control'] : publicGain ? [...replacements, 'control'] : ['control'], [],
    ]);
    const rootParent = clan ? 'clanmingdian' : 'reference_fixture_use';
    assert.deepEqual(after.root, { event: 'recast', parent: rootParent, cards: materials, num: null, cancelled: false, finished: true });
    assert.deepEqual(after.losingEvents, [{
      event: 'loseToDiscardpile', parent: 'recast', cards: materials, num: null, cancelled: false, finished: true,
    }]);
    assert.deepEqual(after.gainingEvents, [{
      event: gainEvent, parent: 'recast', cards: publicGain ? replacements : [], num: publicGain ? null : count,
      cancelled: cancelDraw, finished: true,
    }]);
    assert.deepEqual(after.innerLoss, {
      event: 'lose', parent: 'loseToDiscardpile', cards: cancelLoss ? materials : discarded, num: null,
      cancelled: cancelLoss, finished: true,
    });

    const announce = one(sample, 'display', 'recast-log');
    assert.deepEqual(announce.detail, { cards: materials, inSharedLog: true, spectatorKnownHands: before.spectatorKnownHands });
    const paid = one(sample, 'callback', 'recastingLost');
    const gained = one(sample, 'callback', 'recastingGained');
    for (const [pre, post, child] of [
      ['recastingLose', 'recastingLost', 'loseToDiscardpile'], ['recastingGain', 'recastingGained', gainEvent],
    ]) {
      const requestBefore = one(sample, 'request', pre);
      const factory = one(sample, 'factory', pre);
      const requestAfter = one(sample, 'request', post);
      const callbackBefore = one(sample, 'callback', pre);
      const childBefore = one(sample, 'request', `${child}Before`);
      const callbackAfter = one(sample, 'callback', post);
      assert.ok(ordered(announce, requestBefore, factory, requestAfter, callbackBefore, childBefore, callbackAfter));
      assert.deepEqual(factory.detail, {
        player: 'A', cards: materials, sameArray: true, child: {
          event: child, parent: 'recast', cards: child === 'draw' ? [] : child === 'gain' ? replacements : materials,
          num: child === 'draw' ? count : null, cancelled: false, finished: false,
        },
      });
    }
    assert.deepEqual(one(sample, 'request', 'recastingLost').zones, before.zones);
    assert.deepEqual(one(sample, 'request', 'recastingGained').zones, paid.zones);
    assert.deepEqual(gained.zones, after.zones);
    assert.ok(ordered(paid, one(sample, 'callback', 'recast'), gained, one(sample, 'callback', 'recastAfter')));
    const lossBefore = getRows(sample, 'request', 'loseBefore').filter(row => row.parent === 'loseToDiscardpile');
    assert.deepEqual(lossBefore.map(({ detail, player }) => [detail.cards, detail.visible, player]), [[materials, true, 'A']]);
    const losses = getRows(sample, 'request', 'loseAfter').filter(row => row.parent === 'loseToDiscardpile');
    assert.deepEqual(losses.map(({ detail }) => [detail.stockCards, detail.cards, detail.lostFrom]), cancelLoss ? [] : [
      [materials, discarded, { h: discarded.filter(card => card !== 'equipped'), e: clan ? [] : ['equipped'] }],
    ]);
    if (publicGain) {
      assert.equal(getRows(sample, 'request', 'drawBefore').length, 0);
    } else {
      const drawResult = one(sample, 'request', cancelDraw ? 'drawCancelled' : 'drawAfter');
      assert.deepEqual([drawResult.parent, drawResult.player, drawResult.source, drawResult.detail.num, drawResult.detail.drawn],
        ['recast', 'A', 'A', count, drawn]);
    }
    const gain = getRows(sample, 'request', 'gainAfter').filter(row => row.parent === (publicGain ? 'recast' : 'draw'));
    assert.deepEqual(gain.map(row => row.detail.cards), cancelDraw ? [] : [drawn]);
  }
  const intercept = one('intercept-one', 'probe', 'intercept-returned');
  const gift = getRows('intercept-one', 'request', 'gainAfter').find(row => row.player === 'B');
  assert.deepEqual([gift.source, gift.detail.cards, gift.zones[1]], ['A', ['hidden'], 'B:h']);
  assert.ok(ordered(gift, intercept, one('intercept-one', 'callback', 'recastingLost')));
  assert.deepEqual(result.trials[1].after.recipientKnownHands[1], ['hidden']);
  assert.equal(one('cancel-loss', 'request', 'loseCancelled').parent, 'loseToDiscardpile');

  const choose = one('clan-choice', 'request', 'chooseCardBegin');
  const selected = one('clan-choice', 'callback', 'chooseCardAfter');
  assert.deepEqual(choose.detail, { position: 'he', range: [1, 'unbounded'],
    offered: ['shown', 'control', 'hidden'], eligible: ['shown', 'hidden'], selected: [], accepted: null,
    spectatorKnownHands: [['shown', 'control'], []], discardOrder: [] });
  assert.deepEqual(selected.detail, {
    ...choose.detail, selected: ['shown', 'hidden'], accepted: true, callbackParent: 'trigger',
  });
  const chosenRecast = one('clan-choice', 'request', 'recastBefore');
  assert.equal(selected.parent, 'clanmingdian_cost');
  assert.equal(chosenRecast.parent, 'clanmingdian');
  assert.ok(ordered(choose, selected, chosenRecast));
  assert.deepEqual(chosenRecast.detail.cards, selected.detail.selected);
}
