import assert from 'node:assert/strict';

export function installRestInterruptions() {
  window.__runRestInterruptions = async ({ lib, game, _status, actor: A }) => {
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const label = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    const B = seats[1];
    if (B === game.zhu) throw new Error('Expected B to be a non-lord target');
    if (game.me === B) {
      game.swapPlayer(A);
      B.setIdentity('cai');
    }
    const timings = ['damageBefore', 'damageBegin', 'damageBegin1', 'damageBegin2', 'damageBegin3',
      'damageBegin4', 'damage', 'damageSource', 'damageEnd', 'damageAfter', 'damageCancelled',
      'changeHpBefore', 'changeHpBegin', 'changeHp', 'changeHpEnd', 'changeHpAfter', 'changeHpCancelled', 'rest', 'restEnd'];
    const events = [], trials = [];
    let active = null, hit = null;
    const state = () => ({ hp: B.hp, alive: B.isAlive(), out: B.isOut(), in: B.isIn(),
      clock: _status._rest_return?.[B.playerid] ? { ..._status._rest_return[B.playerid] } : null,
      history: hit ? B.getHistory('damage').includes(hit) : false });
    function record(kind, timing, event) {
      events.push([active, kind, timing, event.name, event.getParent()?.name ?? null,
        ...Object.values(state()), !!event.finished, !!event._cancelled]);
    }
    const observer = 'reference_rest_interrupt_observer', interrupt = 'reference_rest_interrupt';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true, forceOut: true,
      filter: event => !!active && event.player === B,
      async content(event, trigger) { record('callback', event.triggername, trigger); } };
    lib.skill[interrupt] = { trigger: { global: timings }, forced: true, silent: true, forceOut: true, priority: -100,
      filter: (event, player, timing) => !!active && event.player === B && timing === active,
      async content() { await B.rest({ type: 'phase', count: -1 }); } };
    for (const skill of [observer, interrupt]) { game.finishSkill(skill); A.addSkill(skill); }
    const proto = lib.element.GameEvent.prototype, originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (active && this.player === B && timings.includes(timing)) record('request', timing, this);
      return originalTrigger.apply(this, arguments);
    };
    try {
      for (const timing of ['damageBefore', 'damageBegin', 'damageBegin1', 'damageBegin4', 'changeHpBefore']) {
        B.hp = B.maxHp = 4;
        B.hujia = 0;
        B.update();
        active = timing;
        hit = B.damage(1, A);
        await hit;
        const afterDamage = state();
        const finished = !!hit.finished, cancelled = !!hit._cancelled;
        active = null;
        await B.restEnd({ hp: 4 });
        trials.push({ timing, afterDamage, finished, cancelled, afterRestEnd: state() });
      }
      const clockTrials = [], phaseEvents = [];
      let clockTrial = null;
      const clockState = () => ({ hp: B.hp, alive: B.isAlive(), out: B.isOut(), in: B.isIn(),
        clock: _status._rest_return?.[B.playerid] ? { ..._status._rest_return[B.playerid] } : null,
        mark: !!B.marks._rest_return, undist: B.hasSkill('undist'), candidate: game.filterPlayer().includes(B),
        previousNext: label(B.previous.getNext()) });
      const clockObserver = 'reference_rest_clock_phase_observer';
      lib.skill[clockObserver] = { trigger: { global: ['phaseBefore', 'phaseBeforeStart'] }, firstDo: true,
        forced: true, silent: true, forceOut: true,
        filter: () => !!clockTrial,
        async content(event, trigger) { phaseEvents.push([clockTrial, event.triggername, label(trigger.player),
          trigger.skill ?? null, clockState()]); } };
      game.finishSkill(clockObserver);
      A.addSkill(clockObserver);
      for (const [type, count] of [['phase', 2], ['round', 2], ['phase', 0], ['round', 0], ['phase', -1], ['round', -1]]) {
        clockTrial = `${type}:${count}`;
        await B.rest({ type, count });
        await B.die().set('includeOut', true);
        B.revive(2, false);
        const afterRevive = clockState();
        await A.phase('reference_extra').set('phaseList', []);
        const afterOtherExtra = clockState();
        await B.phase('reference_extra').set('phaseList', []);
        const afterOwnExtra = clockState();
        await B.phase().set('phaseList', []);
        const afterOwnNatural = clockState();
        await B.rest({ type: 'phase', count: 9 });
        const afterRestAgain = clockState();
        clockTrial = null;
        await B.restEnd({ hp: 4 });
        clockTrials.push({ type, count, afterRevive, afterOtherExtra, afterOwnExtra, afterOwnNatural, afterRestAgain });
      }
      A.removeSkill(clockObserver);
      return { target: label(B), eventColumns: ['trial', 'kind', 'timing', 'event', 'parent',
        'hp', 'alive', 'out', 'in', 'clock', 'history', 'finished', 'cancelled'], events, trials,
        clockTrials, phaseEvents };
    } finally {
      proto.trigger = originalTrigger;
      A.removeSkill(observer);
      A.removeSkill(interrupt);
    }
  };
}

export function checkRestInterruptions(result) {
  assert.equal(result.target, 'B');
  assert.deepEqual(result.trials.map(trial => trial.timing),
    ['damageBefore', 'damageBegin', 'damageBegin1', 'damageBegin4', 'changeHpBefore']);
  for (const [index, trial] of result.trials.entries()) {
    assert.deepEqual(trial.afterDamage, { hp: 4, alive: true, out: true, in: false,
      clock: { type: 'phase', count: -1 }, history: index >= 2 });
    assert.deepEqual(trial.afterRestEnd, { hp: 4, alive: true, out: false, in: true, clock: null, history: index >= 2 });
    assert.deepEqual([trial.finished, trial.cancelled], [true, false]);
    const requests = result.events.filter(row => row[0] === trial.timing && row[1] === 'request');
    const timings = requests.map(row => row[2]);
    assert.ok(timings.includes('rest') && timings.includes('damageEnd') && timings.includes('damageAfter'));
    assert.equal(timings.includes('changeHpBefore'), index >= 2);
    assert.ok(!timings.includes('damageCancelled') && !timings.includes('changeHpCancelled'));
    if (index >= 2) assert.ok(timings.includes('changeHpEnd') && timings.includes('changeHpAfter'));
  }
  assert.deepEqual(result.clockTrials.map(({ type, count }) => [type, count]),
    [['phase', 2], ['round', 2], ['phase', 0], ['round', 0], ['phase', -1], ['round', -1]]);
  for (const trial of result.clockTrials) {
    const initial = trial.afterRevive;
    assert.deepEqual([initial.hp, initial.alive, initial.in, initial.out, initial.mark, initial.undist,
      initial.candidate, initial.previousNext], [2, true, true, false, false, true, true, 'C']);
    assert.deepEqual(initial.clock, { type: trial.type, count: trial.count });
    const infinite = trial.count <= 0;
    assert.deepEqual(trial.afterOtherExtra.clock,
      infinite || trial.type === 'round' ? initial.clock : null);
    assert.deepEqual(trial.afterOwnExtra.clock,
      infinite || trial.type === 'round' ? initial.clock : null);
    assert.deepEqual(trial.afterOwnNatural.clock, infinite ? initial.clock : null);
    assert.equal(trial.afterOwnNatural.undist, true);
    assert.deepEqual([trial.afterRestAgain.out, trial.afterRestAgain.clock],
      infinite ? [false, initial.clock] : [true, { type: 'phase', count: 9 }]);
  }
}
