import assert from 'node:assert/strict';

export function installCompareMultiple() {
  window.__runCompareMultiple = async ({ lib, game, ui, get, _status, actor: A }) => {
    const C = game.me;
    if (A === C) {
      const hand = C.getCards('h');
      if (hand.length) await C.discard(hand);
      return;
    }
    const opponents = [];
    for (let player = A.next; opponents.length < 3; player = player.next) {
      if (player !== C) opponents.push(player);
    }
    const [B, D, E] = opponents;
    const seats = { A, B, C, D, E };
    const participants = [A, B, D, E];
    const seat = player => player ? Object.keys(seats).find(label => seats[label] === player) || 'other' : null;
    const labels = participants.map(seat);
    const skillPacks = {
      gushe: (await import('/character/sp/skill.js')).default,
      twdanlie: (await import('/character/tw/skill.js')).default,
    };
    for (const [skill, pack] of Object.entries(skillPacks)) {
      if (!lib.skill[skill]) {
        lib.skill[skill] = pack[skill];
        game.finishSkill(skill);
      }
    }
    lib.config.game_speed = 'vvfast';

    const samples = [
      ['pairs-fixing', 'gushe', [7, 8, 10, 12]],
      ['meanwhile-fixing', 'twdanlie', [10, 9, 10, 8]],
      ['meanwhile-tie', 'twdanlie', [10, 11, 11, 8]],
      ['cancel-one-loss', 'gushe', [9, 8, 10, 12]],
      ['cancel-root', 'twdanlie', [10, 9, 10, 8]],
    ];
    const eventColumns = ['sample', 'kind', 'timing', 'event', 'parent', 'player', 'target',
      'zones', 'points', 'detail'];
    const events = [];
    const trials = [];
    let trial;
    const id = card => trial?.cards.get(card) || null;
    const cardId = card => (id(card) ? id(card) : card ? 'other' : null);
    const parent = event => event?.getParent()?.name || null;
    const underRoot = event => !!trial?.root && event.getParent('chooseToCompare', true, true) === trial.root;
    const getCard = label => [...trial.cards].find(([, value]) => value === label)?.[0];
    const bids = labels.map(label => `${label}-bid`);
    const zone = card => {
      const owner = get.owner(card);
      const location = get.position(card, true);
      return owner ? `${seat(owner)}:${location}` : location || null;
    };
    const zones = () => bids.map(label => zone(getCard(label)));
    const known = (owner, viewer) => owner.getKnownCards(viewer).map(id).filter(Boolean).sort();
    const matrix = viewer => participants.map(owner => known(owner, viewer));
    const visible = () => [...ui.arena.querySelectorAll('.card.thrownhighlight:not(.infohidden)')]
      .filter(node => node.getClientRects().length);

    function summarize(result) {
      if (!result) return null;
      return {
        player: cardId(result.player), targets: result.targets?.map(cardId) ?? null,
        num1: result.num1?.slice() ?? null, num2: result.num2?.slice() ?? null,
        ...(Object.hasOwn(result, 'winner') ? { winner: seat(result.winner) } : {}),
        ...Object.fromEntries(['bool', 'tie', 'cancelled'].filter(key => Object.hasOwn(result, key))
          .map(key => [key, result[key] ?? null])),
      };
    }

    function capture(kind, timing, event, detail = null) {
      events.push([
        trial.sample, kind, timing, event.name, parent(event), seat(event.player), seat(event.target), zones(),
        event.num1 != null || event.num2 != null ? [event.num1 ?? null, event.num2 ?? null] : null, detail,
      ]);
    }

    function privateChoice(event) {
      return {
        offered: event.player.getCards(event.position).map(id), selected: event.result?.cards?.map(id) || [],
        chooserKnowledge: matrix(event.player), spectatorKnowledge: matrix(C),
        spectatorFaces: bids.filter(label => visible().some(node => node._cardid === getCard(label).cardid)),
        finished: event.finished,
      };
    }

    const rootTimings = ['chooseToCompareBefore', 'chooseToCompareBegin', 'compareCardShowBefore', 'compare', 'compareFixing',
      'chooseToCompareEnd', 'chooseToCompareAfter', 'chooseToCompareCancelled'];
    const payTimings = ['loseAsyncBegin', 'loseAsyncAfter', 'loseBefore', 'loseAfter', 'loseCancelled', 'cardsDiscardAfter'];
    const requestTimings = new Set([...rootTimings, ...payTimings, 'chooseCardOLBegin', 'chooseCardOLAfter',
      'chooseCardBegin', 'chooseCardAfter', 'showCardsAfter', 'damageAfter', 'loseHpAfter']);
    const callbackTimings = [...rootTimings.slice(2), ...payTimings, 'chooseCardOLAfter', 'showCardsAfter'];

    function watched(event, timing) {
      if (!trial) return false;
      if (underRoot(event)) {
        if (rootTimings.includes(timing)) return event === trial.root;
        return requestTimings.has(timing);
      }
      if (timing === 'cardsDiscardAfter') {
        return parent(event) === 'orderingDiscard' && event.cards?.some(card => id(card)) &&
          event.getParent().relatedEvent === trial.root;
      }
      return ['damageAfter', 'loseHpAfter'].includes(timing) && !!event.getParent(trial.skill, true);
    }

    function detailFor(event, timing, callback) {
      const detail = {};
      if (callback) detail.callbackParent = parent(callback);
      if (event === trial.root) {
        detail.result = summarize(event.result);
        detail.targets = event.targets.map(seat);
        detail.selectedCards = [id(event.card1), id(event.card2)];
        detail.compareMeanwhile = !!event.compareMeanwhile;
        detail.cancelled = !!event._cancelled;
        detail.finished = event.finished;
        if (timing === 'chooseToCompareBefore') detail.fixedResultSupplied = !!event.fixedResult;
        if (['compare', 'compareFixing'].includes(timing)) {
          detail.index = event.iwhile ?? event.iiwhile ?? null;
          detail.internalWinner = seat(event.winner);
        }
      }
      if (event.name === 'chooseCard') return privateChoice(event);
      if (event.name === 'chooseCardOL') {
        detail.participants = event.list.map(seat);
        detail.selected = Array.isArray(event.result) ? event.result.map(choice => choice.cards?.map(id) || []) : [];
      }
      if (event.name === 'loseAsync') {
        detail.batch = event.lose_list.map(([player, cards]) => [seat(player), cards.map(id)]);
      }
      if (event.cards && event !== trial.root) {
        const cards = event.cards.map(id).filter(Boolean);
        if (cards.length) detail.cards = cards;
      }
      if (event.name === 'lose') detail.relatedCompare = event.relatedEvent === trial.root;
      if (event.name === 'showCards') {
        detail.onlyTiming = !!event.triggeronly;
        detail.spectatorKnowledge = matrix(C);
      }
      if (['damageAfter', 'loseHpAfter'].includes(timing)) {
        detail.hp = event.player.hp;
        detail.source = seat(event.source);
      }
      return detail;
    }

    const observer = 'reference_compare_multiple_observer';
    lib.skill[observer] = {
      trigger: { global: callbackTimings }, firstDo: true, forced: true, silent: true,
      filter: (event, player, timing) => watched(event, timing),
      async content(event, trigger) {
        const timing = event.triggername;
        capture('callback', timing, trigger, detailFor(trigger, timing, event));
        if (trial.sample === 'pairs-fixing' && timing === 'compareFixing' && trigger.target === B) {
          trigger.num1 = 10;
          capture('probe', 'modified-fixing', trigger, detailFor(trigger, timing));
        }
        if (trial.sample === 'meanwhile-fixing' && timing === 'compareFixing') {
          if (trigger.player === A) {
            trigger.num1 = 1;
            capture('probe', 'modified-fixing', trigger, detailFor(trigger, timing));
          } else if (trigger.target === D) {
            trigger.num2 = 13;
            capture('probe', 'modified-fixing', trigger, detailFor(trigger, timing));
          }
        }
        if (trial.sample === 'cancel-one-loss' && timing === 'loseBefore' && trigger.player === D) trigger.cancel();
        if (trial.sample === 'cancel-root' && timing === 'compareCardShowBefore') trigger.cancel();
      },
    };
    game.finishSkill(observer);
    A.addSkill(observer);

    const configured = new WeakSet();
    const prefer = (event, { state }) => {
      if (!underRoot(event) || state !== 'begin' || configured.has(event)) return;
      configured.add(event);
      if (event.name === 'chooseCard' && parent(event) === 'chooseCardOL') {
        event.ai = card => id(card) === `${seat(event.player)}-bid` ? 10 : -1;
      }
      if (event.name === 'chooseToDiscard' && parent(event) === 'compareMultiple') event.ai = () => -1;
    };
    for (const name of ['chooseCard', 'chooseToDiscard']) game.globalEventHandlers.pushHandler(name, prefer);
    const contentStates = new WeakSet();
    for (const name of ['compareMultiple', 'twdanlie_add']) {
      game.globalEventHandlers.pushHandler(name, (event, { state }) => {
        if (!underRoot(event) || state === 'begin' && contentStates.has(event)) return;
        if (state === 'begin') contentStates.add(event);
        const root = trial.root;
        const pointEvent = name === 'compareMultiple' ? event : root;
        capture('content', state, event, {
          pairPlayer: seat(pointEvent.player), pairTarget: seat(pointEvent.target),
          cards: [id(pointEvent.card1), id(pointEvent.card2)], points: [pointEvent.num1 ?? null, pointEvent.num2 ?? null],
          winner: seat(pointEvent.winner), gusheMarks: A.countMark('gushe'), result: summarize(root.result),
        });
      });
    }

    const originalTrigger = lib.element.GameEvent.prototype.trigger;
    lib.element.GameEvent.prototype.trigger = function (timing) {
      if (trial && timing === 'chooseToCompareBefore' && parent(this) === trial.skill) trial.root = this;
      if (watched(this, timing)) capture('request', timing, this, detailFor(this, timing));
      return originalTrigger.apply(this, arguments);
    };
    const originalVideo = game.addVideo;
    game.addVideo = function (type, player, info) {
      if (trial?.root && player === A && ['compare', 'compareMultiple'].includes(type)) {
        const root = trial.root;
        const cards = type === 'compare' ? [root.card1, root.card2] : [root.card1, ...root.cardlist];
        const faces = (type === 'compare' ? [info[0], info[2]] : [info[0], ...info[2]])
          .map(face => [face[2], face[0], face[1]]);
        const targets = type === 'compare' ? [seat(root.target)] : root.targets.map(seat);
        trial.reveals.push({ type, targets, cards: cards.map(id), faces, seen: new Set() });
        capture('display', type, root, { cards: cards.map(id), targets });
      }
      return originalVideo.apply(this, arguments);
    };
    const flips = new MutationObserver(records => {
      const display = trial?.reveals.at(-1);
      if (!display) return;
      for (const { target, oldValue } of records) {
        if (!oldValue?.split(' ').includes('infohidden') || !target.matches('.card.thrownhighlight:not(.infohidden)')) continue;
        const label = display.cards.find(key => getCard(key).cardid === target._cardid);
        if (label && target.isConnected && target.getClientRects().length) display.seen.add(label);
      }
    });
    flips.observe(ui.arena, { subtree: true, attributes: true, attributeFilter: ['class'], attributeOldValue: true });

    try {
      for (const [sample, skill, points] of samples) {
        trial = null;
        for (const player of participants) {
          const held = player.getCards('he');
          if (held.length) await player.discard(held);
          player.maxHp = 4;
          player.hp = player === A && skill === 'twdanlie' ? 3 : 4;
          player.hujia = 0;
          player.update();
        }
        A.removeMark('gushe', A.countMark('gushe'), false);
        A.addSkill(skill);
        const cards = new Map();
        const suits = ['heart', 'diamond', 'club', 'spade'];
        for (const [index, player] of participants.entries()) {
          const bid = game.createCard2('tao', suits[index], points[index]);
          const other = game.createCard2('shan', suits[index], 2);
          cards.set(bid, `${labels[index]}-bid`);
          cards.set(other, `${labels[index]}-other`);
          ui.cardPile.prepend(bid, other);
          await player.draw(2);
        }
        const requested = [E, B, D];
        if (!requested.every(target => lib.skill[skill].filterTarget(null, A, target))) {
          throw new Error(`Expected three legal ${skill} targets`);
        }
        trial = { sample, skill, cards, reveals: [], root: null };
        const input = { requested: requested.map(seat), printedPoints: points, hp: participants.map(player => player.hp) };
        window.__referenceStages.push(`${sample}: hands ready`);
        await A.useSkill({ skill, targets: requested });
        const root = trial.root;
        if (!root) throw new Error(`${sample} did not open the original compare event`);
        trials.push({
          sample, skill, input, result: summarize(root.result), cancelled: !!root._cancelled, zones: zones(),
          hp: participants.map(player => player.hp), gusheMarks: A.countMark('gushe'),
          spectatorKnowledge: matrix(C),
          reveals: trial.reveals.map(({ seen, ...reveal }) => ({
            ...reveal, spectatorSawFlip: reveal.cards.filter(label => seen.has(label)),
          })),
        });
        A.removeSkill(skill);
      }
    } finally {
      lib.element.GameEvent.prototype.trigger = originalTrigger;
      game.addVideo = originalVideo;
      flips.disconnect();
      A.removeSkill(observer);
    }
    return { players: game.players.length, online: !!_status.connectMode, seatOrder: labels, spectator: seat(C),
      trials, eventColumns, events };
  };
}

export function checkCompareMultiple(result) {
  assert.equal(result.players, 8);
  assert.equal(result.online, false);
  assert.equal(result.spectator, 'C');
  const participants = ['A', 'B', 'D', 'E'];
  assert.deepEqual(result.seatOrder, participants);
  assert.deepEqual(result.trials.map(({ sample, skill }) => [sample, skill]), [
    ['pairs-fixing', 'gushe'], ['meanwhile-fixing', 'twdanlie'], ['meanwhile-tie', 'twdanlie'],
    ['cancel-one-loss', 'gushe'], ['cancel-root', 'twdanlie'],
  ]);
  const events = result.events.map(row => Object.fromEntries(result.eventColumns.map((column, index) => [column, row[index]])));
  const bids = participants.map(seat => `${seat}-bid`);
  const inHand = participants.map(seat => `${seat}:h`);
  const nobodyKnows = [[], [], [], []];
  const getRows = (sample, kind, timing) => events.filter(row =>
    row.sample === sample && row.kind === kind && (timing == null || row.timing === timing));
  const request = (sample, timing) => getRows(sample, 'request', timing);
  const one = (sample, kind, timing) => {
    const rows = getRows(sample, kind, timing);
    assert.equal(rows.length, 1, `Expected one ${kind}/${timing} in ${sample}`);
    return rows[0];
  };
  const makeResult = (num1, num2) => ({ player: 'A-bid', targets: bids.slice(1), num1, num2 });

  for (const trial of result.trials) {
    const { sample, skill } = trial;
    const start = one(sample, 'request', 'chooseToCompareBefore');
    const collected = one(sample, 'request', 'chooseCardOLAfter');
    const beforePayment = one(sample, 'request', 'loseAsyncBegin');
    const afterPayment = one(sample, 'callback', 'loseAsyncAfter');
    const beforeReveal = one(sample, 'request', 'compareCardShowBefore');
    const cleanup = one(sample, 'request', 'cardsDiscardAfter');
    const lossCancelled = sample === 'cancel-one-loss';
    const paid = lossCancelled ? bids.filter(card => card !== 'D-bid') : bids;
    const processingZones = lossCancelled ? ['o', 'o', 'D:h', 'o'] : ['o', 'o', 'o', 'o'];
    const ordered = (...rows) => rows.every((row, index) => index === 0 || events.indexOf(row) > events.indexOf(rows[index - 1]));

    assert.deepEqual(start.detail.targets, ['E', 'B', 'D']);
    assert.equal(start.parent, skill);
    assert.equal(start.detail.fixedResultSupplied, false);
    assert.deepEqual(collected.detail.participants, participants);
    assert.deepEqual(collected.detail.selected, bids.map(card => [card]));
    assert.deepEqual(request(sample).filter(row => row.event === 'chooseCard').map(row => [row.player, row.timing]),
      participants.flatMap(seat => [[seat, 'chooseCardBegin'], [seat, 'chooseCardAfter']]));
    for (const row of request(sample).filter(row => row.event === 'chooseCard')) {
      const ownCards = [`${row.player}-bid`, `${row.player}-other`];
      assert.deepEqual(row.zones, inHand);
      assert.deepEqual(row.detail, {
        offered: ownCards,
        chooserKnowledge: participants.map(seat => seat === row.player ? ownCards : []),
        selected: row.timing === 'chooseCardAfter' ? [ownCards[0]] : [],
        spectatorKnowledge: nobodyKnows, spectatorFaces: [], finished: row.timing === 'chooseCardAfter',
      });
    }

    assert.deepEqual(beforePayment.zones, inHand);
    assert.deepEqual(beforePayment.detail.batch, participants.map((seat, index) => [seat, [bids[index]]]));
    assert.deepEqual(afterPayment.zones, processingZones);
    assert.deepEqual(request(sample, 'loseBefore').map(row => [
      row.parent, row.player, row.detail.cards, row.detail.relatedCompare,
    ]), participants.map((seat, index) => ['loseAsync', seat, [bids[index]], true]));
    assert.deepEqual(request(sample, 'loseAfter').map(row => row.detail.cards[0]), paid);
    assert.deepEqual(beforeReveal.detail.targets, ['B', 'D', 'E']);
    assert.deepEqual(beforeReveal.zones, processingZones);
    assert.ok(ordered(collected, beforePayment, afterPayment, beforeReveal, cleanup));
    assert.deepEqual([cleanup.parent, cleanup.detail.cards, cleanup.zones],
      ['orderingDiscard', paid, lossCancelled ? ['d', 'd', 'D:h', 'd'] : ['d', 'd', 'd', 'd']]);
    assert.deepEqual(trial.zones, cleanup.zones);
    assert.deepEqual(trial.spectatorKnowledge, nobodyKnows);
    for (const reveal of trial.reveals) assert.deepEqual(reveal.spectatorSawFlip, reveal.cards);
    const displays = getRows(sample, 'display');
    assert.equal(displays.length, trial.reveals.length);
    if (sample !== 'cancel-root') {
      assert.deepEqual(request(sample, 'showCardsAfter').map(row => [row.player, row.detail.cards, row.detail.onlyTiming]),
        participants.map((seat, index) => [seat, [bids[index]], true]));
      assert.ok(displays.every(row => ordered(beforeReveal, row, cleanup)));
    }
    for (const row of getRows(sample, 'callback')) {
      assert.equal(row.detail.callbackParent, 'trigger');
      assert.ok(request(sample, row.timing).some(previous =>
        previous.event === row.event && previous.player === row.player && previous.target === row.target &&
          previous.detail.index === row.detail.index && ordered(previous, row)));
    }
  }

  function checkPairs(sample, comparisons, resolutions, winners) {
    const trial = result.trials.find(trial => trial.sample === sample);
    const targets = participants.slice(1);
    assert.deepEqual(trial.reveals.map(reveal => [reveal.type, reveal.targets, reveal.cards]),
      targets.map((target, index) => ['compare', [target], ['A-bid', bids[index + 1]]]));
    const execution = events.filter(row => row.sample === sample &&
      (row.kind === 'display' || row.kind === 'request' && ['compare', 'compareFixing'].includes(row.timing) ||
        row.kind === 'content' && row.timing === 'begin'));
    assert.deepEqual(execution.map(({ kind, timing, target, points }) => [kind, timing, target, points]),
      targets.flatMap((target, index) => [
        ['display', 'compare', target, comparisons[index]],
        ['request', 'compare', target, comparisons[index]],
        ['request', 'compareFixing', target, comparisons[index]],
        ['content', 'begin', target, resolutions[index]],
      ]));
    const contents = getRows(sample, 'content', 'begin');
    assert.deepEqual(contents.map(row => [
      row.event, row.parent, row.detail.winner, row.detail.cards, row.detail.result.num1, row.detail.result.num2,
    ]), winners.map((winner, index) => [
      'compareMultiple', 'chooseToCompare', winner, ['A-bid', bids[index + 1]],
      resolutions.slice(0, index + 1).map(([num1]) => num1),
      resolutions.slice(0, index + 1).map(([, num2]) => num2),
    ]));
    assert.deepEqual(trial.result, makeResult(resolutions.map(([num1]) => num1), resolutions.map(([, num2]) => num2)));
    assert.equal(trial.gusheMarks, 2);
  }
  checkPairs('pairs-fixing', [[7, 8], [10, 10], [10, 12]], [[10, 8], [10, 10], [10, 12]], ['A', null, 'E']);
  checkPairs('cancel-one-loss', [[9, 8], [9, 10], [9, 12]], [[9, 8], [9, 10], [9, 12]], ['A', 'D', 'E']);
  const lostD = one('cancel-one-loss', 'request', 'loseCancelled');
  assert.deepEqual([lostD.player, lostD.detail.cards, lostD.zones[2]], ['D', ['D-bid'], 'D:h']);
  assert.equal(request('cancel-one-loss', 'showCardsAfter').find(row => row.player === 'D').zones[2], 'D:h');

  for (const [sample, opponentPoints, winner, finalHp] of [
    ['meanwhile-fixing', [9, 10, 8], 'A', [3, 3, 3, 3]],
    ['meanwhile-tie', [11, 11, 8], null, [2, 4, 4, 4]],
  ]) {
    const trial = result.trials.find(trial => trial.sample === sample);
    const compareCalls = request(sample, 'compare');
    const fixingCalls = request(sample, 'compareFixing');
    const scores = { ...makeResult([11, 11, 11], opponentPoints), winner: null };
    assert.deepEqual(trial.reveals.map(reveal => [reveal.type, reveal.targets, reveal.cards]),
      [['compareMultiple', ['B', 'D', 'E'], bids]]);
    assert.deepEqual(compareCalls.map(row => [row.player, row.target, row.points, row.detail.compareMeanwhile]),
      [['A', null, [10, null], true], ...participants.slice(1).map((target, index) =>
        [null, target, [11, opponentPoints[index]], true])]);
    assert.deepEqual(fixingCalls.map(row => [row.player, row.target, row.detail.index]),
      [['A', 'E', 0], [null, 'B', 0], [null, 'D', 1], [null, 'E', 2]]);
    assert.ok(events.indexOf(compareCalls.at(-1)) < events.indexOf(fixingCalls[0]));
    for (const row of fixingCalls) assert.deepEqual(row.detail.result, scores);
    const nativeAddition = getRows(sample, 'content');
    assert.deepEqual(nativeAddition.map(row => [row.event, row.timing, row.detail.points]), [
      ['twdanlie_add', 'begin', [10, null]], ['twdanlie_add', 'end', [11, null]],
    ]);
    assert.deepEqual(trial.result, { ...scores, winner });
    assert.deepEqual(trial.hp, finalHp);
  }
  assert.deepEqual(getRows('meanwhile-fixing', 'probe').map(row => [row.player, row.target, row.points]), [
    ['A', 'E', [1, 8]], [null, 'D', [1, 13]],
  ]);
  assert.deepEqual(request('meanwhile-fixing', 'damageAfter').map(row => [row.player, row.detail.source, row.detail.hp]),
    [['B', 'A', 3], ['D', 'A', 3], ['E', 'A', 3]]);
  const rootCancel = result.trials.find(trial => trial.sample === 'cancel-root');
  assert.equal(rootCancel.cancelled, true);
  assert.deepEqual(rootCancel.result, { ...makeResult([], []), winner: null });
  assert.deepEqual(rootCancel.reveals, []);
  const cancellationRequest = one('cancel-root', 'request', 'chooseToCompareCancelled');
  const cancellationCallback = one('cancel-root', 'callback', 'chooseToCompareCancelled');
  assert.deepEqual([cancellationRequest.detail.finished, cancellationCallback.detail.finished], [false, true]);
  assert.ok(!request('cancel-root').some(row => ['showCardsAfter', 'compare', 'compareFixing',
    'chooseToCompareEnd', 'chooseToCompareAfter'].includes(row.timing)));
  for (const sample of ['meanwhile-tie', 'cancel-root']) {
    const loss = one(sample, 'request', 'loseHpAfter');
    assert.deepEqual([loss.player, loss.parent, loss.detail.hp, loss.zones], ['A', 'twdanlie', 2, ['d', 'd', 'd', 'd']]);
  }
}
