import assert from 'node:assert/strict';

// Fixture operations call the pinned upstream death and revival methods.
export function installReferenceRevival() {
  window.__runReferenceRevival = async ({ lib, game, ui, get, _status, actor: A }) => {
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    if (new Set(seats).size !== 8 || seats.at(-1).nextSeat !== A) throw new Error('Expected eight physical seats');
    // The upstream live array can start at a different physical seat between
    // browser launches. Rotate it once at setup to compare later appends.
    const start = game.players.indexOf(A);
    if (start < 0) throw new Error('Current player is missing from the live array');
    game.players.push(...game.players.splice(0, start));
    if (game.players.some((player, index) => player !== seats[index])) throw new Error('Expected a seat-ordered live array');
    const label = player => player ? 'ABCDEFGH'[seats.indexOf(player)] || null : null;
    // Fix the hidden inputs while preserving the identity distribution. The
    // seeded startup can consume random values in a different order, so its
    // initial identity shuffle alone does not fix these two physical seats.
    const B = seats[1], C = seats[3];
    if (_status.currentPhase !== A || B === game.zhu || C === game.zhu) {
      throw new Error('Expected B and D to be non-lord seats in the current turn');
    }
    if (game.me === B || game.me === C) {
      const previousHuman = game.me;
      game.swapPlayer(A);
      previousHuman.setIdentity('cai');
    }
    const fixIdentity = (player, identity, fixed) => {
      if (player.identity === identity) return;
      const donor = seats.find(other => other !== player && other !== A && other !== game.me &&
        other !== game.zhu && !fixed.includes(other) && !other.identityShown && other.identity === identity);
      if (!donor || player.identityShown) throw new Error(`Cannot arrange hidden ${identity} identity`);
      [player.identity, donor.identity] = [donor.identity, player.identity];
      player.setIdentity('cai');
      donor.setIdentity('cai');
    };
    fixIdentity(B, 'zhong', []);
    fixIdentity(C, 'fan', [B]);
    if (B === game.me || C === game.me) throw new Error('Expected two non-lord AI targets');
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
    let active = null, cancelRevive = false, activeRevive = null, earlyRecord = null;
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
      if (earlyRecord) earlyRecord(timing, this);
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

      // Exercise the real gushe mark, whose intro does not define onunmark.
      lib.skill.gushe = (await import('/character/sp/skill.js')).default.gushe;
      game.finishSkill('gushe');
      B.hp = B.maxHp = 4;
      B.update();
      B.addSkill('gushe');
      B.addMark('gushe', 7, false);
      const gusheStages = [];
      const captureGushe = name => gusheStages.push({ name, hp: B.hp, alive: B.isAlive(),
        count: B.countMark('gushe'), shown: !!B.marks.gushe, hasSkill: B.hasSkill('gushe'), ...roster() });
      captureGushe('before-die');
      await B.die().set('animate', false);
      captureGushe('after-die');
      B.revive(2, false);
      captureGushe('after-revive');
      const gushe = { hasOnUnmark: !!lib.skill.gushe.intro.onunmark, stages: gusheStages };

      // A different non-lord AI is revived inside the real die trigger, before
      // the death content's cleanup and mode reward steps have completed.
      if (C.identity !== 'fan') throw new Error('Expected the arranged non-lord AI rebel');
      const existingC = C.getCards('he');
      if (existingC.length) await C.discard(existingC);
      C.hp = C.maxHp = 4;
      C.hujia = 0;
      C.update();
      C.addSkill('gushe');
      C.addMark('gushe', 7, false);
      lib.skill.reference_early_temp = {};
      game.finishSkill('reference_early_temp');
      C.addTempSkill('reference_early_temp');
      const earlyHand = game.createCard2('shan', 'diamond', 6);
      const earlyEquip = game.createCard2('qinggang', 'spade', 6);
      await C.gain(earlyHand);
      await C.equip(earlyEquip);
      const rewardCards = [8, 9, 10].map(number => game.createCard2('tao', 'heart', number));
      for (const item of rewardCards.toReversed()) ui.cardPile.insertBefore(item, ui.cardPile.firstChild);
      const sourceCards = A.getCards('h').slice();
      const rewardOrder = [];
      const captureRewardOrder = timing => rewardOrder.push([timing, A.getCards('h').map(item => {
        const bonus = rewardCards.indexOf(item);
        if (bonus !== -1) return `bonus${bonus}`;
        const original = sourceCards.indexOf(item);
        if (original !== -1) return `old${original}`;
        throw new Error('Unexpected card in the reward source hand');
      })]);
      const earlyStages = [];
      const captureEarly = name => earlyStages.push({
        name, hp: C.hp, maxHp: C.maxHp, alive: C.isAlive(), identity: C.identity,
        identityShown: !!C.identityShown, hand: C.getCards('h').map(card), equips: C.getCards('e').map(card),
        markCount: C.countMark('gushe'), markShown: !!C.marks.gushe,
        tempSkill: !!C.tempSkills.reference_early_temp && C.hasSkill('reference_early_temp'),
        sourceHand: A.countCards('h'), sourceKill: A.getStat('kill') || 0,
        rewardsInHand: rewardCards.filter(item => A.getCards('h').includes(item)).map(card),
        ...roster(), gameOver: !!_status.over,
      });
      const earlySkill = 'reference_die_early_revive';
      lib.skill[earlySkill] = {
        trigger: { global: 'die' }, firstDo: true, forced: true, silent: true, forceDie: true,
        filter: event => event.player === C,
        async content() {
          captureEarly('callback-before-revive');
          C.revive(2, false);
          captureEarly('callback-after-revive');
        },
      };
      game.finishSkill(earlySkill);
      A.addSkill(earlySkill);
      captureEarly('before-die');
      earlyRecord = (timing, event) => {
        if (event.player === C && event.name === 'die' &&
          ['dieBefore', 'dieBegin', 'die', 'dieEnd', 'dieAfter'].includes(timing)) {
          captureEarly(`request-${timing}`);
        }
        if (event.player === A && event.name === 'draw' && event.getParent()?.name === 'die' &&
          ['drawBefore', 'drawAfter'].includes(timing)) {
          captureEarly(`request-${timing}`);
          captureRewardOrder(timing);
        }
      };
      await C.die({ source: A }).set('animate', false);
      earlyRecord = null;
      captureEarly('after-die');
      A.removeSkill(earlySkill);
      const earlyRevival = { target: label(C), targetNonLordAI: C !== game.zhu && C !== game.me,
        stages: earlyStages, rewardOrder, handInDiscard: ui.discardPile.contains(earlyHand),
        equipmentInDiscard: ui.discardPile.contains(earlyEquip) };

      // Independent trials leave the original trace untouched. Observe the
      // event from A and from B's own ordinary and forceDie skills.
      const revivalTimings = ['reviveBefore', 'reviveBegin', 'reviveEnd', 'reviveAfter', 'reviveCancelled'];
      const trialObserver = 'reference_revive_trial_observer';
      const trialCancel = 'reference_revive_trial_cancel';
      const ownOrdinary = 'reference_revive_own_ordinary';
      const ownForceDie = 'reference_revive_own_force_die';
      let trialRows = null, cancelAt = null;
      const recordTrial = (kind, timing, event) => trialRows.push([kind, timing, B.hp, B.isAlive(),
        game.players.map(label), game.dead.map(label), !!event.finished, !!event._cancelled]);
      lib.skill[trialObserver] = {
        trigger: { global: revivalTimings }, firstDo: true, forced: true, silent: true, forceDie: true,
        filter: event => !!trialRows && event.player === B,
        async content(event, trigger) { recordTrial('callback', event.triggername, trigger); },
      };
      lib.skill[trialCancel] = {
        trigger: { global: revivalTimings }, forced: true, silent: true, forceDie: true, priority: -100,
        filter: (event, player, name) => !!trialRows && event.player === B && name === cancelAt,
        async content(event, trigger) { trigger.cancel(); },
      };
      for (const [name, forceDie] of [[ownOrdinary, false], [ownForceDie, true]]) {
        lib.skill[name] = {
          trigger: { player: ['reviveBefore', 'reviveBegin', 'reviveEnd'] }, forced: true, silent: true, forceDie,
          filter: event => !!trialRows && event.player === B,
          async content(event, trigger) { recordTrial(forceDie ? 'own-force-die' : 'own-ordinary', event.triggername, trigger); },
        };
      }
      for (const skill of [trialObserver, trialCancel, ownOrdinary, ownForceDie]) game.finishSkill(skill);
      A.addSkill(trialObserver);
      A.addSkill(trialCancel);
      B.addSkill(ownOrdinary);
      B.addSkill(ownForceDie);
      const cancellationTrials = [];
      for (const timing of revivalTimings.slice(0, 4)) {
        if (!B.isAlive()) B.revive(2, false);
        B.hp = B.maxHp = 4;
        B.update();
        await B.die().set('animate', false);
        trialRows = [];
        cancelAt = timing;
        earlyRecord = (name, event) => {
          if (event.player === B && event.name === 'revive' && revivalTimings.includes(name)) recordTrial('request', name, event);
        };
        const revival = B.reviveEvent(2, false);
        await revival;
        earlyRecord = null;
        cancellationTrials.push({ cancelAt: timing, rows: trialRows, hp: B.hp, alive: B.isAlive(),
          players: game.players.map(label), dead: game.dead.map(label),
          finished: !!revival.finished, cancelled: !!revival._cancelled });
        trialRows = null;
      }
      A.removeSkill(trialObserver);
      A.removeSkill(trialCancel);
      B.removeSkill(ownOrdinary);
      B.removeSkill(ownForceDie);
      return { setup, directReturnedUndefined: directReturn === undefined, stages, cancelled, restored,
        eventColumns: ['trial', 'kind', 'timing', 'event', 'parent', 'num', 'eventHp', 'hp', 'maxHp',
          'alive', 'players', 'dead', 'finished', 'cancelled'], events,
        physicalCards: { handInDiscard: ui.discardPile.contains(hand), equipmentInDiscard: ui.discardPile.contains(equipment) },
        gushe, earlyRevival, cancellationTrials };
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

  assert.equal(result.gushe.hasOnUnmark, false);
  assert.deepEqual(result.gushe.stages.map(stage => [stage.name, stage.hp, stage.alive,
    stage.count, stage.shown, stage.hasSkill]), [
    ['before-die', 4, true, 7, true, true],
    ['after-die', 0, false, 7, false, true],
    ['after-revive', 2, true, 7, false, true],
  ]);
  assert.deepEqual(result.gushe.stages.map(stage => stage.dead), [[], [target], []]);
  assert.equal(result.earlyRevival.targetNonLordAI, true);
  const early = result.earlyRevival.stages;
  assert.deepEqual(early.map(stage => stage.name), [
    'before-die', 'request-dieBefore', 'request-dieBegin', 'request-die',
    'callback-before-revive', 'callback-after-revive', 'request-drawBefore', 'request-drawAfter',
    'request-dieEnd', 'request-dieAfter', 'after-die',
  ]);
  const earlyTarget = result.earlyRevival.target, original = early[0];
  const reward = [8, 9, 10].map(number => ['tao', 'heart', number]);
  const oldCards = Array.from({ length: original.sourceHand }, (_, index) => `old${index}`);
  const bonusCards = reward.map((_, index) => `bonus${index}`);
  assert.deepEqual(result.earlyRevival.rewardOrder, [
    ['drawBefore', oldCards],
    ['drawAfter', [...bonusCards, ...oldCards]],
  ]);
  assert.equal(original.identityShown, false);
  for (const [index, stage] of early.entries()) {
    const dead = index === 3 || index === 4;
    const revived = index >= 5;
    const rewarded = index >= 7;
    assert.deepEqual([stage.hp, stage.maxHp, stage.alive], [dead ? 0 : revived ? 2 : 4, 4, !dead], stage.name);
    assert.equal(stage.identity, 'fan', stage.name);
    assert.equal(stage.identityShown, index >= 3, stage.name);
    assert.deepEqual(stage.hand, [['shan', 'diamond', 6]], stage.name);
    assert.deepEqual(stage.equips, [['qinggang', 'spade', 6]], stage.name);
    assert.deepEqual([stage.markCount, stage.markShown, stage.tempSkill], [7, true, true], stage.name);
    assert.equal(stage.sourceKill, original.sourceKill + (index >= 3 ? 1 : 0), stage.name);
    assert.equal(stage.sourceHand, original.sourceHand + (rewarded ? 3 : 0), stage.name);
    assert.deepEqual(stage.rewardsInHand, rewarded ? reward : [], stage.name);
    assert.deepEqual(stage.dead, dead ? [earlyTarget] : [], stage.name);
    assert.deepEqual(stage.players, dead ? original.players.filter(seat => seat !== earlyTarget)
      : revived ? [...original.players.filter(seat => seat !== earlyTarget), earlyTarget] : original.players, stage.name);
    assert.deepEqual(stage.ring, original.ring.filter(seat => !dead || seat !== earlyTarget), stage.name);
    assert.equal(stage.gameOver, false, stage.name);
  }
  assert.equal(result.earlyRevival.handInDiscard, false);
  assert.equal(result.earlyRevival.equipmentInDiscard, false);

  const trialTimings = [
    ['reviveBefore', 'reviveCancelled'],
    ['reviveBefore', 'reviveBegin', 'reviveCancelled'],
    ['reviveBefore', 'reviveBegin', 'reviveEnd', 'reviveCancelled'],
    ['reviveBefore', 'reviveBegin', 'reviveEnd', 'reviveAfter', 'reviveCancelled'],
  ];
  assert.deepEqual(result.cancellationTrials.map(trial => trial.cancelAt),
    ['reviveBefore', 'reviveBegin', 'reviveEnd', 'reviveAfter']);
  for (const [index, trial] of result.cancellationTrials.entries()) {
    const alive = index >= 2;
    for (const kind of ['request', 'callback']) {
      assert.deepEqual(trial.rows.filter(row => row[0] === kind).map(row => row[1]), trialTimings[index]);
    }
    assert.deepEqual([trial.hp, trial.alive, trial.finished, trial.cancelled], [alive ? 2 : 0, alive, true, true]);
    assert.deepEqual(trial.dead, alive ? [] : [target]);
    assert.deepEqual(trial.players, [...original.players.filter(seat => seat !== target && seat !== earlyTarget),
      earlyTarget, ...(alive ? [target] : [])]);
    assert.deepEqual(trial.rows.filter(row => row[0] === 'own-force-die').map(row => row[1]),
      [[], ['reviveBefore'], ['reviveBefore', 'reviveBegin'], ['reviveBefore', 'reviveBegin', 'reviveEnd']][index]);
    assert.deepEqual(trial.rows.filter(row => row[0] === 'own-ordinary').map(row => row[1]),
      index === 3 ? ['reviveEnd'] : []);
  }
}
