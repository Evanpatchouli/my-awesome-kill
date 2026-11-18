import assert from 'node:assert/strict';

export function installPhaseScheduling() {
  window.__installPhaseScheduling = async ({ lib, game, ui, get, _status }) => {
    const yijiang = (await import('/character/yijiang/skill.js')).default;
    const shenhua = (await import('/character/shenhua/skill.js')).default;
    for (const name of ['dangxian', 'fangquan', 'fangquan2', 'fangquan3']) {
      if (!lib.skill[name]) {
        lib.skill[name] = (name === 'dangxian' ? yijiang : shenhua)[name];
        game.finishSkill(name);
      }
    }

    const seatNames = 'ABCDEFGH';
    const turns = [];
    const turnIds = new WeakMap();
    const events = [];
    const cards = new Map();
    const setup = {};
    let seats;
    let complete = false;
    const seat = player => player ? seatNames[seats.indexOf(player)] || null : null;
    const id = card => cards.get(card) || null;
    const fixtureCard = name => [...cards].find(([, label]) => label === name)?.[0];
    const createCard = (label, name, suit, number) => {
      const card = game.createCard2(name, suit, number);
      cards.set(card, label);
      return card;
    };
    const zone = card => {
      const position = get.position(card, true);
      const owner = get.owner(card);
      return owner ? `${seat(owner)}:${position}` : position || null;
    };
    const shaHand = player => player.getCards('h').filter(card => id(card)?.startsWith('sha'));
    const handMaterial = player => player.getCards('h').find(card => id(card) === `cost-${seat(player)}`);
    const materialZones = () => Object.fromEntries([...cards].map(([card, name]) => [name, zone(card)]));
    const rootOf = event => event.getParent('phase', true, true);
    const stageOf = event => event.getParent(item => item.phaseIndex != null, true, true);
    const parentName = event => event.getParent()?.name || null;
    const stageColumns = ['index', 'extraReason', 'skipFlag', 'cancelled', 'finished'];
    const flagColumns = ['roundStart', 'historyRound', 'historyMe', 'historySkipped',
      'statRound', 'statMe', 'statSkipped', 'turnedOver', 'phaseSkipped'];
    const stateColumns = ['flags', 'skippedStages', 'shaStat', 'shaHistory', 'grantPending', 'grantedTurn', 'personalPhaseNumber'];

    const ownerTurns = new Map();
    function getTurn(root) {
      if (!root) return null;
      if (!turnIds.has(root)) {
        const owner = seat(root.player);
        const count = (ownerTurns.get(owner) || 0) + 1;
        ownerTurns.set(owner, count);
        const turn = { id: `${owner}${count}`, owner, source: root.skill || null, parent: parentName(root) };
        turnIds.set(root, turn);
        turns.push(turn);
      }
      return turnIds.get(root);
    }

    function capture(kind, timing, event, callback) {
      const root = rootOf(event);
      const turn = getTurn(root);
      const stage = stageOf(event);
      const player = root?.player;
      const stat = player?.getStat();
      const history = player?.getHistory();
      const state = player ? [
        [
          root._roundStart ? 1 : 0, history?.isRound ? 1 : 0, history?.isMe ? 1 : 0, history?.isSkipped ? 1 : 0,
          stat?.isRound ? 1 : 0, stat?.isMe ? 1 : 0, stat?.isSkipped ? 1 : 0,
          player.isTurnedOver() ? 1 : 0, player.phaseSkipped ? 1 : 0,
        ],
        history?.skipped.length ? history.skipped.slice() : null,
        stat?.card.sha || 0, player.countUsed('sha', true),
        player.countMark('fangquan2'), player.hasSkill('fangquan3') ? 1 : 0, player.phaseNumber,
      ] : null;
      const detail = {};
      if (event.name === 'phase') {
        if (timing === 'phaseBefore') detail.naturalLoopSeat = seat(event.getParent().player);
        if (timing === 'phaseBegin' || timing === 'phaseEnd') detail.phaseList = event.phaseList?.slice();
        if (timing === 'phaseChange') detail.nextStage = event.phaseList[event.num];
      }
      if (event.name === 'phaseDraw' && stage) detail.drawCount = event.num;
      if (event.name === 'phaseJudge' && event.player === seats[7]) {
        detail.delayedCardZone = zone(fixtureCard('lebu-H'));
      }
      if (event.name === 'judge') {
        detail.revealedCard = id(event.result?.card || event.player.judging[0]);
        detail.accepted = event.result?.bool ?? null;
      }
      if (event.name === 'lebu') detail.queuedSkips = event.player.skipList.slice();
      if (event.name === 'phaseDiscard' && stage) {
        detail.handCount = event.player.countCards('h');
        detail.required = event.num ?? null;
      }
      if (timing === 'chooseToUseBegin') {
        const prior = event.addCount_extra;
        detail.shaCandidates = shaHand(event.player).map(card => [
          id(card), event.filterCard(card, event.player, event),
        ]);
        if (prior === undefined) delete event.addCount_extra;
        else event.addCount_extra = prior;
      }
      if (event.name === 'chooseToDiscard') {
        detail.position = event.position || 'h';
        detail.forced = !!event.forced;
        const material = fixtureCard(`cost-${seat(event.player)}`);
        if (material) detail.material = [id(material), zone(material)];
      }
      if (timing === 'chooseTargetBegin') {
        detail.forced = !!event.forced;
        detail.candidates = seats.filter(target => event.filterTarget(null, event.player, target)).map(seat);
      }
      if (event.name.startsWith('choose') && timing.endsWith('After')) {
        detail.accepted = event.result?.bool ?? null;
        detail.cards = event.result?.cards?.map(id).filter(Boolean) || [];
        detail.targets = event.result?.targets?.map(seat) || [];
      }
      const tracked = event.cards?.filter(card => cards.has(card)) || [];
      if (tracked.length) detail.zones = tracked.map(card => [id(card), zone(card)]);
      if (event.name === 'lose' && tracked.some(card => id(card).startsWith('cost-'))) {
        detail.paymentChoice = parentName(event.getParent());
      }
      if (event.name === 'useCard') {
        detail.targets = event.targets.map(seat);
        detail.targetHp = seats[7].hp;
      }
      if (event.name === 'dangxian') detail.phaseList = root.phaseList.slice();
      if (event.name === 'fangquan2' && event.targets?.length) {
        const target = event.targets[0];
        detail.recipient = seat(target);
        detail.recipientMarked = !!target.marks.fangquan;
        detail.recipientSkill = target.hasSkill('fangquan3');
        detail.queuedPhases = root.parent.next.filter(child => child.name === 'phase').map(child => [
          seat(child.player), child.skill || null,
        ]);
      }
      if (event.name === 'fangquan3') detail.recipientMarked = !!event.player.marks.fangquan;
      if (callback) detail.callbackParent = parentName(callback);
      events.push([
        kind, timing, event.name, seat(event.player), parentName(event), turn?.id || null,
        game.phaseNumber, game.roundNumber, seat(_status.currentPhase),
        stage ? [stage.phaseIndex, stage._extraPhaseReason || null, stage.isSkipped ? 1 : 0,
          stage._cancelled ? 1 : 0, stage.finished ? 1 : 0] : null,
        state, Object.keys(detail).length ? detail : null,
      ]);
    }

    const coreTurnTimings = ['phaseBefore', 'phaseBegin', 'phaseEnd', 'phaseAfter', 'phaseCancelled'];
    const boundaryTimings = ['phaseBeforeStart', 'phaseBeforeEnd', 'phaseBeginStart', 'phaseChange'];
    const stageNames = ['phaseZhunbei', 'phaseJudge', 'phaseDraw', 'phaseUse', 'phaseDiscard', 'phaseJieshu'];
    const stageTimings = stageNames.flatMap(name => ['Before', 'Begin', '', 'End', 'After', 'Skipped', 'Cancelled', 'Omitted']
      .map(suffix => name + suffix));
    const choiceTimings = ['chooseToUse', 'chooseBool', 'chooseToDiscard', 'chooseTarget']
      .flatMap(name => ['Begin', 'After'].map(suffix => name + suffix));
    const movementTimings = ['loseBefore', 'loseAfter', 'cardsDiscardAfter', 'gainAfter', 'useCard2', 'useCardAfter'];
    const allTimings = ['roundStart', 'roundEnd', 'phaseOver', 'turnOverAfter', 'phaseDrawBegin1', 'phaseDrawBegin2',
      'judge', 'judgeFixing', ...coreTurnTimings, ...boundaryTimings, ...stageTimings, ...choiceTimings, ...movementTimings];

    function watched(event, timing) {
      if (!seats || complete) return false;
      if (['phaseOver', 'roundEnd', 'roundStart'].includes(timing)) return true;
      const root = rootOf(event);
      if (!root) return false;
      if (!setup.zones && event !== root) return false;
      if (event === root && coreTurnTimings.includes(timing)) return true;
      const name = seat(root.player);
      if (name === 'A' && (boundaryTimings.includes(timing) || stageTimings.includes(timing) ||
          ['phaseDrawBegin1', 'phaseDrawBegin2'].includes(timing))) return true;
      if (event.name === 'phaseUse' && ['B', 'C', 'D', 'E', 'H'].includes(name) && stageTimings.includes(timing)) return true;
      if (name === 'C' && event.name === 'phaseDraw' && stageTimings.includes(timing)) return true;
      if (name === 'G' && ['phaseBeforeStart', 'phaseBeforeEnd'].includes(timing)) return true;
      if (name === 'H' && (event.name === 'phaseJudge' && stageTimings.includes(timing) ||
          ['judge', 'judgeFixing'].includes(timing))) return true;
      if (timing === 'phaseDrawSkipped' || timing === 'turnOverAfter') return true;
      if (choiceTimings.includes(timing)) {
        const parent = parentName(event);
        if (parent === 'phaseUse' || parent === 'phaseDiscard') return name === 'A';
        return parent === 'fangquan_cost' || parent === 'fangquan2_cost';
      }
      return movementTimings.includes(timing) && !!event.cards?.some(card => cards.has(card));
    }

    const contentStarts = new WeakSet();
    for (const name of ['dangxian', 'fangquan_cost', 'fangquan', 'fangquan2_cost', 'fangquan2', 'fangquan3', 'lebu']) {
      game.globalEventHandlers.pushHandler(name, (event, { state }) => {
        if (!seats || complete) return;
        if (state === 'begin') {
          if (contentStarts.has(event)) return;
          contentStarts.add(event);
        }
        capture('content', state, event);
      });
    }

    // Choose only fixture-scripted preferences through upstream AI. The existing
    // filters, results, card costs and scheduling content remain in place.
    const configured = new WeakSet();
    function preferChoice(event, { state }) {
      if (!seats || complete || state !== 'begin' || configured.has(event)) return;
      configured.add(event);
      const parent = parentName(event);
      const name = seat(event.player);
      if (event.name === 'chooseToUse' && parent === 'phaseUse') {
        event.ai1 = card => name === 'A' && id(card)?.startsWith('sha') ? 20 - get.number(card) : -10;
        event.ai2 = target => name === 'A' && target === seats[7] ? 10 : -10;
      } else if (event.name === 'chooseBool' && parent === 'fangquan_cost') {
        event.ai = () => name !== 'C';
      } else if (event.name === 'chooseToDiscard' && parent === 'fangquan2_cost') {
        const material = handMaterial(event.player);
        event.ai = card => name !== 'D' && card === material ? 10 : -10;
      } else if (event.name === 'chooseToDiscard' && parent === 'phaseDiscard' && name === 'A') {
        event.ai = card => id(card) === 'draw-1' ? 20 : id(card)?.startsWith('sha') ? -10 : 1;
      } else if (event.name === 'chooseTarget' && parent === 'fangquan2_cost') {
        const recipient = name === 'B' ? seats[0] : seats[6];
        event.ai = target => target === recipient ? 10 : -10;
      }
    }
    for (const name of ['chooseToUse', 'chooseBool', 'chooseToDiscard', 'chooseTarget']) {
      game.globalEventHandlers.pushHandler(name, preferChoice);
    }

    game.globalEventHandlers.pushHandler('phaseLoop', (event, { state }) => {
      if (seats || state !== 'begin') return;
      seats = [event.player];
      while (seats.length < game.players.length) seats.push(seats.at(-1).next);
      if (seats.length !== 8 || new Set(seats).size !== 8) throw new Error('Expected the eight-seat identity table');
      setup.browserSeat = seat(game.me);
      setup.seats = seatNames.split('');
      if (!_status.auto) ui.click.auto('forced');
      lib.config.game_speed = 'vvfast';
      seats.forEach(player => { player.noPhaseDelay = true; });
      seats[0].addSkill('dangxian');
      for (const player of seats.slice(1, 5)) player.addSkill('fangquan');
      seats[2].skip('phaseDraw');
    });

    async function arrangeOpening() {
      for (const player of seats) {
        const initial = player.getCards('hej');
        if (initial.length) await player.discard(initial);
        player.maxHp = 9;
        player.hp = 9;
        player.hujia = 0;
        player.update();
      }
      const [A, B, C, D, E, , G, H] = seats;
      A.hp = 4;
      A.update();
      for (const number of [3, 4, 5, 6, 7]) createCard(`sha-${number}`, 'sha', 'spade', number);
      await A.gain([...cards.keys()], 'draw');
      for (const player of [B, C, D, E]) {
        const material = createCard(`cost-${seat(player)}`, 'tao', 'heart', seatNames.indexOf(seat(player)) + 3);
        await player.gain(material, 'draw');
      }
      await G.turnOver(true);
      await H.addJudge(createCard('lebu-H', 'lebu', 'club', 6));
      const draw1 = createCard('draw-1', 'tao', 'heart', 2);
      const draw2 = createCard('draw-2', 'tao', 'heart', 3);
      ui.cardPile.prepend(draw1, draw2);
      ui.special.appendChild(createCard('judge-H', 'sha', 'spade', 8));
      setup.targetHp = H.hp;
      setup.cards = Object.fromEntries([...cards].map(([card, name]) => [name, [card.name, get.suit(card), get.number(card)]]));
      setup.zones = materialZones();
      setup.turnedOver = seat(G);
    }

    const observer = 'reference_phase_scheduling_observer';
    lib.skill[observer] = {
      trigger: { global: allTimings }, firstDo: true, forced: true, silent: true,
      filter: (event, player, timing) => player === game.me && watched(event, timing),
      async content(event, trigger) {
        try {
          const timing = event.triggername;
          capture('callback', timing, trigger, event);
          const turn = turnIds.get(rootOf(trigger));
          if (timing === 'phaseBefore' && turn?.id === 'A1') await arrangeOpening();
          if (timing === 'phaseBefore' && turn?.id === 'H1') ui.cardPile.prepend(fixtureCard('judge-H'));
          if (timing === 'phaseBegin' && turn?.id === 'A3') {
            complete = true;
            window.__referenceResult = {
              setup, turns,
              eventColumns: ['kind', 'timing', 'event', 'seat', 'parent', 'turn', 'phaseNumber',
                'roundNumber', 'currentSeat', 'stage', 'state', 'detail'],
              stageColumns, flagColumns, stateColumns, events,
              final: { targetHp: seats[7].hp, zones: materialZones() },
            };
            game.pause();
          }
        } catch (error) {
          window.__referenceError = `phase-scheduling observer: ${error.stack || error}`;
        }
      },
    };
    game.finishSkill(observer);
    game.addGlobalSkill(observer);
    const proto = lib.element.GameEvent.prototype;
    const originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (watched(this, timing)) capture('request', timing, this);
      return originalTrigger.apply(this, arguments);
    };
  };
}

export function checkPhaseScheduling(result) {
  assert.equal(result.setup.seats.length, 8);
  const turnOrder = ['A1', 'B1', 'A2', 'C1', 'D1', 'E1', 'G1', 'F1', 'G2', 'H1', 'A3'];
  assert.deepEqual(result.turns, turnOrder.map(id => ({
    id, owner: id[0], source: ['A2', 'G1'].includes(id) ? 'fangquan2' : null, parent: 'phaseLoop',
  })));

  function named(columns, values) {
    return values ? Object.fromEntries(columns.map((column, index) => [column, values[index]])) : null;
  }
  const timeline = result.events.map(row => {
    const event = named(result.eventColumns, row);
    const state = named(result.stateColumns, event.state);
    return { ...event, stage: named(result.stageColumns, event.stage), state,
      flags: named(result.flagColumns, state?.flags) };
  });
  const requests = timeline.filter(row => row.kind === 'request');
  const callbacks = timeline.filter(row => row.kind === 'callback');
  const find = (rows, turn, timing) => {
    const row = rows.find(event => event.turn === turn && event.timing === timing);
    assert.ok(row, `Expected ${timing} in ${turn}`);
    return row;
  };
  const compareKey = row => [row.timing, row.event, row.seat, row.turn, row.stage?.index].join('/');
  assert.deepEqual(callbacks.map(compareKey).sort(), requests.map(compareKey).sort());
  assert.ok(callbacks.every(row => row.detail.callbackParent === 'trigger'));

  assert.deepEqual(requests.filter(row => row.timing === 'phaseBefore').map(row => [
    row.turn, row.detail.naturalLoopSeat, row.phaseNumber, row.roundNumber, row.currentSeat,
  ]), turnOrder.map((turn, index) => [
    turn, turn === 'A2' ? 'B' : turn === 'G1' ? 'E' : turn[0], index, index === 0 ? 0 : 1, null,
  ]));
  assert.deepEqual(requests.filter(row => row.timing === 'roundStart').map(row => [
    row.turn, row.phaseNumber, row.roundNumber, row.currentSeat, row.flags.roundStart,
    row.flags.historyRound, row.flags.statRound, row.flags.historyMe, row.flags.statMe,
  ]), [['A1', 1, 1, null, 1, 1, 1, 0, 0], ['A3', 11, 2, null, 1, 1, 1, 0, 0]]);
  assert.deepEqual(requests.filter(row => row.timing === 'roundEnd').map(row => [
    row.event, row.seat, row.phaseNumber, row.roundNumber, row.currentSeat,
  ]), [['phaseLoop', 'A', 10, 1, null]]);
  assert.deepEqual(requests.filter(row => row.timing === 'phaseOver').map(row => [
    row.seat, row.phaseNumber, row.currentSeat,
  ]), [...'ABCDEFGH'].map((seat, index) => [seat, [1, 3, 4, 5, 7, 8, 9, 10][index], null]));
  assert.ok(requests.filter(row => row.timing === 'phaseBegin').every(row =>
    row.seat === row.currentSeat && row.flags.historyMe && row.flags.statMe));

  const phases = ['phaseZhunbei', 'phaseJudge', 'phaseDraw', 'phaseUse', 'phaseDiscard', 'phaseJieshu'];
  const inserted = ['phaseUse|dangxian', ...phases];
  for (const turn of ['A1', 'A2']) {
    assert.deepEqual(find(callbacks, turn, 'phaseBegin').detail.phaseList, phases);
    assert.deepEqual(requests.filter(row => row.turn === turn && row.timing === 'phaseChange')
      .map(row => row.detail.nextStage), inserted);
    assert.deepEqual(timeline.filter(row => row.turn === turn && row.event === 'dangxian')
      .map(row => [row.kind, row.timing, row.detail.phaseList]), [
      ['content', 'begin', phases], ['content', 'end', inserted],
    ]);
    assert.deepEqual(requests.filter(row => row.turn === turn && row.timing === 'phaseUseBefore').map(row => [
      row.parent, row.stage.index, row.stage.extraReason, row.state.shaStat, row.state.shaHistory,
    ]), [['phase', 0, 'dangxian', 0, 0], ['phase', 4, null, 0, 1]]);
  }
  const candidates = requests.filter(row => row.timing === 'chooseToUseBegin');
  assert.equal(candidates.length, 8);
  for (const [index, row] of candidates.entries()) {
    assert.equal(row.parent, 'phaseUse');
    assert.equal(row.state.shaStat, index % 2);
    assert.ok(row.detail.shaCandidates.length);
    assert.ok(row.detail.shaCandidates.every(([, allowed]) => allowed === (index % 2 === 0)));
  }
  const used = ['sha-3', 'sha-4', 'sha-5', 'sha-6'];
  assert.deepEqual(requests.filter(row => row.timing === 'useCardAfter').map(row => [
    row.turn, row.stage.index, row.state.shaStat, row.state.shaHistory,
    row.detail.targets, row.detail.zones, row.detail.targetHp,
  ]), used.map((card, index) => [
    index < 2 ? 'A1' : 'A2', index % 2 ? 4 : 0, 1, index % 2 + 1,
    ['H'], [[card, 'o']], result.setup.targetHp - index - 1,
  ]));
  for (const card of used) {
    const moves = requests.filter(row => row.detail?.zones?.some(([label]) => label === card));
    assert.deepEqual(moves.filter(row => ['loseAfter', 'cardsDiscardAfter'].includes(row.timing))
      .map(row => [row.timing, row.detail.zones]), [
      ['loseAfter', [[card, 'o']]], ['cardsDiscardAfter', [[card, 'd']]],
    ]);
    assert.equal(result.final.zones[card], 'd');
  }
  assert.equal(result.final.zones['sha-7'], 'A:h');
  assert.equal(result.final.targetHp, result.setup.targetHp - 4);

  assert.deepEqual(find(requests, 'A1', 'gainAfter').detail.zones, [['draw-1', 'A:h'], ['draw-2', 'A:h']]);
  const discard = find(requests, 'A1', 'chooseToDiscardAfter');
  assert.deepEqual([discard.parent, discard.detail.forced, discard.detail.cards], ['phaseDiscard', true, ['draw-1']]);
  assert.equal(result.final.zones['draw-1'], 'd');
  assert.deepEqual(timeline.filter(row => row.event === 'lebu').map(row => [
    row.turn, row.timing, row.parent, row.detail.queuedSkips,
  ]), [['H1', 'begin', 'phaseJudge', []], ['H1', 'end', 'phaseJudge', ['phaseUse']]]);
  assert.deepEqual(find(callbacks, 'H1', 'judgeFixing').detail, {
    revealedCard: 'judge-H', accepted: false, callbackParent: 'trigger',
  });
  for (const [turn, stage] of [['C1', 'phaseDraw'], ['H1', 'phaseUse']]) {
    const timings = requests.filter(row => row.turn === turn && row.event === stage);
    assert.deepEqual(timings.map(row => row.timing), [`${stage}Skipped`]);
    assert.deepEqual(timings[0].state.skippedStages, [stage]);
    assert.deepEqual([timings[0].stage.cancelled, timings[0].stage.finished], [0, 1]);
  }

  assert.deepEqual(requests.filter(row => row.timing === 'chooseBoolAfter').map(row => [
    row.seat, row.parent, row.detail.accepted,
  ]), [['B', 'fangquan_cost', true], ['C', 'fangquan_cost', false],
    ['D', 'fangquan_cost', true], ['E', 'fangquan_cost', true]]);
  for (const turn of ['B1', 'D1', 'E1']) {
    assert.deepEqual(requests.filter(row => row.turn === turn && row.event === 'phaseUse')
      .map(row => row.timing), ['phaseUseBefore', 'phaseUseCancelled']);
    const cancelled = find(requests, turn, 'phaseUseCancelled');
    const observed = find(callbacks, turn, 'phaseUseCancelled');
    assert.deepEqual(cancelled.state.skippedStages, ['phaseUse']);
    assert.deepEqual([cancelled.stage.cancelled, cancelled.stage.finished, cancelled.state.grantPending], [1, 0, 0]);
    assert.deepEqual([observed.stage.cancelled, observed.stage.finished, observed.state.grantPending], [1, 1, 1]);
  }
  assert.ok(find(requests, 'C1', 'phaseUseBegin'));
  assert.ok(find(requests, 'C1', 'phaseUseAfter'));
  assert.equal(result.final.zones['cost-C'], 'C:h');

  const refused = find(requests, 'D1', 'chooseToDiscardAfter');
  assert.deepEqual([refused.detail.accepted, refused.detail.material], [false, ['cost-D', 'D:h']]);
  assert.ok(!requests.some(row => row.turn === 'D1' && row.timing === 'chooseTargetBegin'));
  assert.equal(find(requests, 'D1', 'phaseAfter').state.grantPending, 1);
  assert.equal(find(callbacks, 'D1', 'phaseAfter').state.grantPending, 0);
  assert.equal(result.final.zones['cost-D'], 'D:h');

  for (const [grant, recipient, extra, following] of [['B1', 'A', 'A2', 'C1'], ['E1', 'G', 'G1', 'F1']]) {
    const material = `cost-${grant[0]}`;
    assert.deepEqual(requests.filter(row => row.detail?.zones?.some(([label]) => label === material))
      .map(row => [row.timing, row.parent, row.detail.paymentChoice, row.detail.zones]), [
      ['loseBefore', 'discard', 'chooseToDiscard', [[material, `${grant[0]}:h`]]],
      ['loseAfter', 'discard', 'chooseToDiscard', [[material, 'd']]],
    ]);
    const paid = find(requests, grant, 'chooseToDiscardAfter');
    const select = find(requests, grant, 'chooseTargetBegin');
    assert.deepEqual([paid.detail.accepted, paid.detail.material], [true, [material, 'd']]);
    assert.ok(timeline.indexOf(paid) < timeline.indexOf(select));
    assert.deepEqual(select.detail, { forced: true, candidates: result.setup.seats.filter(seat => seat !== grant[0]) });
    assert.deepEqual(find(requests, grant, 'chooseTargetAfter').detail.targets, [recipient]);
    const queued = timeline.find(row => row.turn === grant && row.event === 'fangquan2' && row.timing === 'end');
    assert.deepEqual(queued.detail, { recipient, recipientMarked: true, recipientSkill: true,
      queuedPhases: [[grant[0], null], [recipient, 'fangquan2']] });
    const after = find(requests, grant, 'phaseAfter');
    const start = find(requests, extra, 'phaseBefore');
    const loop = requests.find(row => row.timing === 'phaseOver' && row.seat === grant[0]);
    const next = find(requests, following, 'phaseBefore');
    const order = [after, start, loop, next].map(row => timeline.indexOf(row));
    assert.ok(order.every((index, step) => index >= 0 && (step === 0 || index > order[step - 1])));
    assert.equal(start.state.grantedTurn, 1);
    assert.equal(result.final.zones[material], 'd');
  }
  const cancelled = find(requests, 'G1', 'phaseCancelled');
  const flipped = find(callbacks, 'G1', 'phaseCancelled');
  assert.deepEqual([cancelled.flags.turnedOver, flipped.flags.turnedOver], [1, 0]);
  assert.deepEqual([cancelled.flags.historySkipped, cancelled.flags.statSkipped, cancelled.flags.phaseSkipped,
    cancelled.phaseNumber, cancelled.state.personalPhaseNumber, cancelled.roundNumber, cancelled.currentSeat],
  [1, 1, 1, 7, 0, 1, null]);
  assert.ok(!requests.some(row => row.turn === 'G1' && ['phaseBegin', 'phaseEnd', 'phaseAfter'].includes(row.timing)));
  assert.equal(find(requests, 'G2', 'phaseBegin').state.personalPhaseNumber, 1);
  assert.deepEqual(timeline.filter(row => row.event === 'fangquan3').map(row => [
    row.turn, row.timing, row.detail.recipientMarked, row.state.grantedTurn,
  ]), [['A2', 'begin', true, 1], ['A2', 'end', false, 0], ['G1', 'begin', true, 1], ['G1', 'end', false, 0]]);
}
