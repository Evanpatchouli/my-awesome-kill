import assert from 'node:assert/strict';

// All rule operations below are the pinned upstream methods and skill contents.
export function installReferenceMaxHp() {
  window.__runReferenceMaxHp = async ({ lib, game, ui, get, _status, actor: A }) => {
    const candidates = game.players.filter(player => player !== A && player !== game.me && player !== game.zhu);
    const [B, C, D] = candidates;
    if (!B || !C || !D || _status.currentPhase !== A) throw new Error('Expected three distinct non-lord AI seats');
    const seats = { A, B, C, D };
    const label = player => Object.keys(seats).find(key => seats[key] === player) || (player ? 'other' : null);
    const state = player => [player.hp, player.maxHp, player.isAlive(), player.countCards('h')];
    const fields = subject => ({
      num: subject.num ?? null, source: label(subject.source), originalHp: subject.originalHp ?? null,
      originalMaxHp: subject.originalMaxHp ?? null, changedHp: subject.changedHp ?? null,
      changedMaxHp: subject.changedMaxHp ?? null, loseHp: subject.loseHp ?? null,
      finished: subject.finished, cancelled: !!subject._cancelled,
    });
    for (const player of game.players) {
      const cards = player.getCards('he');
      if (cards.length) await player.discard(cards);
      player.hp = player.maxHp = 4;
      player.hujia = 0;
      player.update();
    }
    const setup = {
      players: game.players.length, actorCurrent: _status.currentPhase === A,
      nonLordAI: [B, C, D].every(player => player !== game.zhu && player !== game.me),
      roles: Object.fromEntries(Object.entries(seats).map(([name, player]) => [name, player.identity])),
    };
    const times = [
      ...['gainMaxHp', 'loseMaxHp', 'changeHp', 'die', 'dying'].flatMap(name =>
        ['Before', 'Begin', 'End', 'After', 'Cancelled'].map(suffix => name + suffix)),
      'changeHp', 'die', 'dying', 'damageBefore', 'damageBegin4', 'damage', 'damageEnd',
      'damageAfter', 'damageCancelled', 'drawBefore', 'drawEnd', 'drawAfter',
    ];
    const events = [], trials = [];
    let activeName = null, activeSeat = null, activeEvent = null, cancelAt = null;
    function record(kind, timing, event) {
      events.push([activeName, kind, timing, event.name, event.getParent()?.name || null,
        label(event.player), label(event.source), event.num ?? null, ...state(activeSeat),
        event.originalHp ?? null, event.originalMaxHp ?? null, event.changedHp ?? null,
        event.changedMaxHp ?? null, event.loseHp ?? null, !!event.finished, !!event._cancelled,
        event.reason?.name || null, label(event.reason?.source)]);
    }
    const observer = 'reference_max_hp_observer', cancel = 'reference_max_hp_cancel';
    lib.skill[observer] = {
      trigger: { global: times }, firstDo: true, forced: true, silent: true, forceDie: true,
      filter: event => !!activeSeat && event.player === activeSeat,
      async content(event, trigger) { record('callback', event.triggername, trigger); },
    };
    lib.skill[cancel] = {
      trigger: { global: times }, forced: true, silent: true, forceDie: true,
      filter: (event, player, timing) => !!cancelAt && timing === cancelAt &&
        (event === activeEvent || (event.name === 'die' && event.getParent() === activeEvent)),
      async content(event, trigger) { trigger.cancel(); },
    };
    game.finishSkill(observer);
    game.finishSkill(cancel);
    A.addSkill(observer);
    A.addSkill(cancel);
    const proto = lib.element.GameEvent.prototype;
    const original = proto.trigger;
    proto.trigger = function (timing) {
      if (activeSeat && this.player === activeSeat && times.includes(timing)) record('request', timing, this);
      return original.apply(this, arguments);
    };
    const observeContent = (event, { state }) => {
      if (activeSeat && event.player === activeSeat) record('content-' + state, event.name, event);
    };
    for (const skill of ['shiyong', 'weizhong']) game.globalEventHandlers.pushHandler(skill, observeContent);
    async function run(name, seat, method, num, hp, maxHp, source, cancellation = null) {
      seat.maxHp = maxHp;
      seat.hp = hp;
      seat.update();
      activeName = name;
      activeSeat = seat;
      cancelAt = cancellation;
      const before = state(seat);
      const handCounts = name.startsWith('weizhong-')
        ? game.players.map(player => player.countCards('h')).sort((a, b) => a - b) : null;
      const minimum = handCounts ? seat.isMinHandcard() : null;
      const priorHand = handCounts ? seat.getCards('h').slice() : null;
      activeEvent = seat[method]({ num });
      if (source) activeEvent.source = source;
      await activeEvent;
      trials.push({ name, method, requested: num, before, after: state(seat), event: fields(activeEvent),
        cancelAt: cancellation, gameOver: !!_status.over,
        ...(handCounts ? { handCounts, minimum, drawn: seat.getCards('h').filter(card => !priorHand.includes(card))
          .map(card => [card.name, get.suit(card), get.number(card)]) } : {}) });
      activeEvent = null;
      cancelAt = null;
      activeSeat = null;
    }
    try {
      await run('gain', B, 'gainMaxHp', 1, 4, 4, A);
      await run('lose-full', B, 'loseMaxHp', 1, 4, 4, A);
      await run('lose-injured', B, 'loseMaxHp', 1, 2, 4, A);
      await run('gain-zero', B, 'gainMaxHp', 0, 3, 4, A);
      await run('lose-zero', B, 'loseMaxHp', 0, 3, 4, A);
      await run('gain-negative', B, 'gainMaxHp', -2, 3, 4, A);
      await run('lose-negative', B, 'loseMaxHp', -2, 3, 4, A);
      await run('gain-negative-below-zero', B, 'gainMaxHp', -5, 4, 4, A);
      await run('gain-from-negative-hp', B, 'gainMaxHp', 1, -1, 4, A);
      await run('lose-from-negative-hp', B, 'loseMaxHp', 1, -1, 4, A);
      await run('gain-cancel-before', B, 'gainMaxHp', 1, 4, 4, A, 'gainMaxHpBefore');
      await run('gain-cancel-end', B, 'gainMaxHp', 1, 4, 4, A, 'gainMaxHpEnd');
      await run('lose-cancel-begin', B, 'loseMaxHp', 1, 4, 4, A, 'loseMaxHpBegin');
      await run('lose-cancel-end', B, 'loseMaxHp', 1, 4, 4, A, 'loseMaxHpEnd');
      await run('die-cancel-before', B, 'loseMaxHp', 1, 1, 1, A, 'dieBefore');

      if (!lib.skill.weizhong) lib.skill.weizhong = (await import('/character/sp/skill.js')).default.weizhong;
      if (!lib.skill.shiyong) lib.skill.shiyong = (await import('/character/yijiang/skill.js')).default.shiyong;
      game.finishSkill('weizhong');
      game.finishSkill('shiyong');
      const draws = [7, 8, 9].map(number => game.createCard2('tao', 'heart', number));
      for (const card of draws.toReversed()) ui.cardPile.insertBefore(card, ui.cardPile.firstChild);
      B.addSkill('weizhong');
      await run('weizhong-tied-min', B, 'gainMaxHp', 1, 4, 4, A);
      await run('weizhong-not-min', B, 'loseMaxHp', 1, 4, 4, A);
      B.removeSkill('weizhong');
      const held = B.getCards('h');
      if (held.length) await B.discard(held);
      B.maxHp = B.hp = 4;
      B.update();
      B.addSkill('shiyong');
      const sha = game.createCard2('sha', 'heart', 4);
      await A.gain(sha);
      activeName = 'shiyong-red-sha';
      activeSeat = B;
      const before = state(B);
      const canUse = A.canUse(sha, B);
      await A.useCard(sha, B, false);
      trials.push({ name: activeName, before, after: state(B), canUse,
        shaInDiscard: ui.discardPile.contains(sha), gameOver: !!_status.over });
      activeSeat = null;
      B.removeSkill('shiyong');
      await run('death-zero-no-source', C, 'loseMaxHp', 1, 1, 1, null);
      await run('death-negative-source', D, 'loseMaxHp', 2, 1, 1, A);
      return { setup, remaining: game.players.length, dead: game.dead.length, trials,
        eventColumns: ['trial', 'kind', 'timing', 'subject', 'parent', 'player', 'source', 'num',
          'hp', 'maxHp', 'alive', 'hand', 'originalHp', 'originalMaxHp', 'changedHp', 'changedMaxHp',
          'loseHp', 'finished', 'cancelled', 'reason', 'reasonSource'], events };
    } finally {
      proto.trigger = original;
      A.removeSkill(observer);
      A.removeSkill(cancel);
      B.removeSkill('weizhong');
      B.removeSkill('shiyong');
      for (const skill of ['shiyong', 'weizhong']) {
        game.globalEventHandlers.removeHandler(skill, game.globalEventHandlers.getDefaultHandlerType(skill), observeContent);
      }
    }
  };
}

export function checkReferenceMaxHp(result) {
  assert.equal(result.setup.players, 8);
  assert.equal(result.setup.actorCurrent, true);
  assert.equal(result.setup.nonLordAI, true);
  assert.equal(result.remaining, 6);
  assert.equal(result.dead, 2);
  const trials = Object.fromEntries(result.trials.map(trial => [trial.name, trial]));
  const checks = {
    gain: [4, 5, true, 0], 'lose-full': [3, 3, true, 0], 'lose-injured': [2, 3, true, 0],
    'gain-zero': [3, 4, true, 0], 'lose-zero': [3, 4, true, 0],
    'gain-negative': [2, 2, true, 0], 'lose-negative': [3, 6, true, 0],
    'gain-negative-below-zero': [-1, -1, true, 0],
    'gain-from-negative-hp': [-1, 5, true, 0], 'lose-from-negative-hp': [-1, 3, true, 0],
    'gain-cancel-before': [4, 4, true, 0], 'gain-cancel-end': [4, 5, true, 0],
    'lose-cancel-begin': [4, 4, true, 0], 'lose-cancel-end': [3, 3, true, 0],
    'die-cancel-before': [0, 0, true, 0],
    'weizhong-tied-min': [4, 5, true, 2], 'weizhong-not-min': [3, 3, true, 3],
    'shiyong-red-sha': [3, 3, true, 0],
    'death-zero-no-source': [0, 0, false, 0], 'death-negative-source': [-1, -1, false, 0],
  };
  assert.deepEqual(Object.keys(trials), Object.keys(checks));
  for (const [name, after] of Object.entries(checks)) {
    assert.deepEqual(trials[name].after, after, name);
    assert.equal(trials[name].gameOver, false, name);
    if (trials[name].event) {
      assert.equal(trials[name].event.num, trials[name].requested, name);
      assert.equal(trials[name].event.source, name === 'death-zero-no-source' ? null : 'A', name);
      assert.equal(trials[name].event.finished, true, name);
    }
  }
  for (const [name, originalHp, originalMaxHp, changedHp, changedMaxHp, loseHp] of [
    ['gain', 4, 4, 0, 1, null], ['lose-full', 4, 4, -1, -1, 1],
    ['lose-injured', 2, 4, 0, -1, 0], ['gain-zero', 3, 4, 0, 0, null],
    ['lose-zero', 3, 4, 0, 0, 0], ['gain-negative', 3, 4, 0, -2, null],
    ['lose-negative', 3, 4, 0, 2, 0], ['gain-negative-below-zero', 4, 4, 0, -5, null],
    ['gain-from-negative-hp', 0, 4, 0, 1, null], ['lose-from-negative-hp', 0, 4, 0, -1, 0],
    ['death-zero-no-source', 1, 1, -1, -1, 1], ['death-negative-source', 1, 1, -1, -2, 2],
  ]) {
    const event = trials[name].event;
    assert.deepEqual([event.originalHp, event.originalMaxHp, event.changedHp, event.changedMaxHp, event.loseHp],
      [originalHp, originalMaxHp, changedHp, changedMaxHp, loseHp], name);
  }
  const rows = result.events.map(row => Object.fromEntries(result.eventColumns.map((key, index) => [key, row[index]])));
  const requests = name => rows.filter(row => row.trial === name && row.kind === 'request').map(row => row.timing);
  for (const [name, expected] of Object.entries({
    gain: ['gainMaxHpBefore', 'gainMaxHpBegin', 'gainMaxHpEnd', 'gainMaxHpAfter'],
    'lose-full': ['loseMaxHpBefore', 'loseMaxHpBegin', 'loseMaxHpEnd', 'loseMaxHpAfter'],
    'gain-zero': ['gainMaxHpBefore', 'gainMaxHpBegin', 'gainMaxHpEnd', 'gainMaxHpAfter'],
    'lose-zero': ['loseMaxHpBefore', 'loseMaxHpBegin', 'loseMaxHpEnd', 'loseMaxHpAfter'],
    'gain-cancel-before': ['gainMaxHpBefore', 'gainMaxHpCancelled'],
    'gain-cancel-end': ['gainMaxHpBefore', 'gainMaxHpBegin', 'gainMaxHpEnd', 'gainMaxHpCancelled'],
    'lose-cancel-begin': ['loseMaxHpBefore', 'loseMaxHpBegin', 'loseMaxHpCancelled'],
    'lose-cancel-end': ['loseMaxHpBefore', 'loseMaxHpBegin', 'loseMaxHpEnd', 'loseMaxHpCancelled'],
    'die-cancel-before': ['loseMaxHpBefore', 'loseMaxHpBegin', 'dieBefore', 'dieCancelled', 'loseMaxHpEnd', 'loseMaxHpAfter'],
    'death-zero-no-source': ['loseMaxHpBefore', 'loseMaxHpBegin', 'dieBefore', 'dieBegin', 'die', 'dieEnd', 'dieAfter', 'loseMaxHpEnd', 'loseMaxHpAfter'],
    'death-negative-source': ['loseMaxHpBefore', 'loseMaxHpBegin', 'dieBefore', 'dieBegin',
      'changeHpBefore', 'changeHpBegin', 'changeHp', 'changeHpEnd', 'changeHpAfter', 'die', 'dieEnd', 'dieAfter', 'loseMaxHpEnd', 'loseMaxHpAfter'],
  })) {
    assert.deepEqual(requests(name), expected, name);
    assert.deepEqual(rows.filter(row => row.trial === name && row.kind === 'callback').map(row => row.timing), expected, name);
  }
  for (const name of ['gain-cancel-before', 'gain-cancel-end', 'lose-cancel-begin', 'lose-cancel-end']) {
    assert.equal(trials[name].event.cancelled, true, name);
  }
  for (const name of ['gain-cancel-before', 'lose-cancel-begin']) {
    assert.equal(trials[name].event.originalHp, null, name);
    assert.equal(trials[name].event.originalMaxHp, null, name);
  }
  assert.equal(trials['gain-cancel-end'].event.changedMaxHp, 1);
  assert.equal(trials['lose-cancel-end'].event.changedMaxHp, -1);
  assert.equal(trials['die-cancel-before'].event.cancelled, false);
  const cancelledDeath = rows.filter(row => row.trial === 'die-cancel-before' && row.timing === 'dieCancelled');
  assert.deepEqual(cancelledDeath.map(row => [row.kind, row.finished, row.cancelled]),
    [['request', false, true], ['callback', true, true]]);
  for (const name of ['death-zero-no-source', 'death-negative-source']) {
    const source = name === 'death-negative-source' ? 'A' : null;
    const death = rows.find(row => row.trial === name && row.kind === 'request' && row.timing === 'dieBefore');
    assert.equal(death.parent, 'loseMaxHp');
    assert.equal(death.reason, 'loseMaxHp');
    assert.equal(death.source, source);
    assert.equal(death.reasonSource, source);
    assert.equal(trials[name].event.source, source);
  }
  assert.deepEqual(trials['weizhong-tied-min'].handCounts, Array(8).fill(0));
  assert.equal(trials['weizhong-tied-min'].minimum, true);
  assert.deepEqual(trials['weizhong-tied-min'].drawn, [['tao', 'heart', 7], ['tao', 'heart', 8]]);
  assert.deepEqual(trials['weizhong-not-min'].handCounts, [0, 0, 0, 0, 0, 0, 0, 2]);
  assert.equal(trials['weizhong-not-min'].minimum, false);
  assert.deepEqual(trials['weizhong-not-min'].drawn, [['tao', 'heart', 9]]);
  for (const [name, amount, timing] of [['weizhong-tied-min', 2, 'gainMaxHpEnd'], ['weizhong-not-min', 1, 'loseMaxHpEnd']]) {
    const events = rows.filter(row => row.trial === name);
    const index = (kind, time) => events.findIndex(row => row.kind === kind && row.timing === time);
    for (const [kind, time] of [['request', timing], ['content-begin', 'weizhong'], ['request', 'drawBefore'],
      ['request', 'drawAfter'], ['content-end', 'weizhong'], ['request', timing.replace('End', 'After')]]) {
      assert.ok(index(kind, time) >= 0, `${name}: ${kind} ${time}`);
    }
    assert.ok(index('request', timing) < index('content-begin', 'weizhong'));
    assert.ok(index('content-begin', 'weizhong') < index('request', 'drawBefore'));
    assert.ok(index('request', 'drawAfter') < index('content-end', 'weizhong'));
    assert.ok(index('content-end', 'weizhong') < index('request', timing.replace('End', 'After')));
    assert.equal(events.find(row => row.kind === 'request' && row.timing === 'drawBefore').num, amount);
  }
  assert.equal(trials['shiyong-red-sha'].canUse, true);
  assert.equal(trials['shiyong-red-sha'].shaInDiscard, true);
  const shiyong = rows.filter(row => row.trial === 'shiyong-red-sha');
  const index = (kind, timing) => shiyong.findIndex(row => row.kind === kind && row.timing === timing);
  for (const [kind, timing] of [['request', 'damageEnd'], ['content-begin', 'shiyong'],
    ['request', 'loseMaxHpBefore'], ['request', 'loseMaxHpAfter'], ['content-end', 'shiyong'], ['request', 'damageAfter']]) {
    assert.ok(index(kind, timing) >= 0, `shiyong: ${kind} ${timing}`);
  }
  assert.ok(index('request', 'damageEnd') < index('content-begin', 'shiyong'));
  assert.ok(index('content-begin', 'shiyong') < index('request', 'loseMaxHpBefore'));
  assert.ok(index('request', 'loseMaxHpAfter') < index('content-end', 'shiyong'));
  assert.ok(index('content-end', 'shiyong') < index('request', 'damageAfter'));
  assert.equal(shiyong.find(row => row.kind === 'request' && row.timing === 'loseMaxHpBefore').parent, 'shiyong');
}
