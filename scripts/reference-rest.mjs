import assert from 'node:assert/strict';

export function installReferenceRest() {
  window.__installReferenceRest = async ({ lib, game, ui, _status }) => {
    const { mbdanggu, mbmowang } = (await import('/character/mobile/skill.js')).default;
    for (const [skill, definition] of Object.entries({ mbdanggu, mbmowang })) {
      if (!lib.skill[skill]) {
        lib.skill[skill] = definition;
        game.finishSkill(skill);
      }
    }

    const labels = [...'ABCDEFGH'];
    const subjectNames = ['B', 'D', 'E', 'F', 'G'];
    const restTimings = ['rest', 'restEnd', 'revive'].flatMap(name =>
      ['', 'Before', 'Begin', 'End', 'After', 'Cancelled'].map(suffix => name + suffix));
    const lifecycleTimings = [...restTimings, 'dieBefore', 'dieBegin', 'die', 'dieEnd', 'dieAfter', 'dieCancelled'];
    const phaseTimings = ['phaseBeforeStart', 'phaseBegin', 'phaseOver', 'phaseUseBegin'];
    const eventColumns = ['kind', 'trial', 'timing', 'seat', 'turn', 'parent', 'data'];
    const stateColumns = ['hp', 'maxHp', 'alive', 'out', 'in', 'clock', 'mark', 'undist', 'hand', 'equip',
      'linked', 'turnedOver', 'inPlayers', 'inDead', 'counted', 'previous', 'next', 'previousEligibleNext', 'identityShown'];
    const events = [], stages = [], turns = [];
    let seats, trial = null, active = false, protocolDone = false, roundClockStarted = false, realCards;
    let dNaturalTurns = 0, eNaturalTurns = 0, cancel = null, competingClocks = false;
    const seat = player => labels[seats.indexOf(player)] ?? null;
    const [A, B, C, D, E, F, G] = [0, 1, 2, 3, 4, 5, 6];
    const pendingPhaseTurns = ['C-extra', 'B-extra', 'B-natural', 'C-natural'];
    const turnName = event => event.skill || `${seat(event.player)}-natural`;
    const clock = player => {
      const entry = _status._rest_return?.[player.playerid];
      return entry ? [entry.type, entry.count] : null;
    };
    const state = player => [player.hp, player.maxHp, player.isAlive(), player.isOut(), player.isIn(),
      clock(player), !!player.marks._rest_return, player.hasSkill('undist'), player.countCards('h'),
      player.countCards('e'), player.isLinked(), player.isTurnedOver(), game.players.includes(player),
      game.dead.includes(player), game.filterPlayer().includes(player), seat(player.previous), seat(player.next),
      seat(player.previous.getNext()), !!player.identityShown];
    const capture = (name, player, detail = null) => {
      stages.push({ name, seat: seat(player), state: state(player), detail });
    };
    const watched = (event, timing) => active && lifecycleTimings.includes(timing) &&
      subjectNames.includes(seat(event.player));

    function observe(kind, timing, event) {
      const phase = event.getParent('phase', true, true);
      events.push([kind, trial, timing, seat(event.player), phase ? turnName(phase) : null,
        event.getParent()?.name ?? null, {
          hp: event.player.hp, out: event.player.isOut(), alive: event.player.isAlive(), clock: clock(event.player),
          reviveHp: event.hp ?? null, reserveOut: !!event.reserveOut, finished: event.finished,
          cancelled: !!event._cancelled,
        }]);
    }

    async function setupPlayer(player) {
      const cards = player.getCards('he');
      if (cards.length) await player.discard(cards);
      setVitals(player);
      const hand = game.createCard2('sha', 'spade', 7);
      const equip = game.createCard2('zhuge', 'club', 1);
      await player.gain(hand);
      await player.equip(equip);
      await player.link(true);
      await player.turnOver(true);
      return [hand, equip];
    }

    function setVitals(player) {
      player.hp = 2;
      player.maxHp = 4;
      player.update();
    }

    async function directTrials() {
      const player = seats[B];
      const [hand, equip] = await setupPlayer(player);
      active = true;
      capture('initial', player);
      const run = async (name, makeEvent, detail) => {
        trial = name;
        const event = makeEvent();
        await event;
        capture(name, player, { finished: event.finished, cancelled: !!event._cancelled, ...detail?.() });
      };
      await run('end-without-rest', () => player.restEnd({ hp: 1 }));
      cancel = 'rest';
      await run('cancel-rest', () => player.rest());
      cancel = null;
      await run('repeat-rest', () => player.rest({ type: 'round', count: 8 }));
      cancel = 'reviveBefore';
      await run('cancel-revive', () => player.restEnd({ hp: 3 }), () => ({
        handRetained: player.getCards('h').includes(hand), equipRetained: player.getCards('e').includes(equip),
      }));
      cancel = null;
      await run('zero-clock', () => player.rest({ type: 'phase', count: 0 }));
      await run('manual-zero-hp', () => player.restEnd({ hp: 0 }));
      await run('negative-clock', () => player.rest({ type: 'phase', count: -2 }));
      cancel = 'restEnd';
      await run('cancel-rest-end', () => player.restEnd(), () => ({
        handRetained: player.getCards('h').includes(hand), equipRetained: player.getCards('e').includes(equip),
      }));
      cancel = null;
      await run('repeat-end', () => player.restEnd({ hp: 2 }));
      await player.turnOver(false);
      trial = 'persistent-clocks';
      setVitals(seats[F]);
      setVitals(seats[G]);
      await seats[F].rest({ type: 'phase', count: 0 });
      await seats[G].rest({ type: 'round', count: -2 });
      capture('persistent-zero-placed', seats[F]);
      capture('persistent-negative-placed', seats[G]);
      await run('phase-clock', () => player.rest({ type: 'phase', count: 4 }));
      seats[C].insertPhase('C-extra');
      seats[B].insertPhase('B-extra');
      protocolDone = true;
    }

    function turnState(timing, event) {
      const player = event.player;
      const turn = turnName(event);
      // Keep the relevant seat boundaries and global round transitions. Each
      // sample includes the other subjects' clocks at that same boundary.
      if (timing !== 'roundStart' && !subjectNames.includes(seat(player)) && !pendingPhaseTurns.includes(turn)) return;
      turns.push({ timing, turn, phase: game.phaseNumber, round: game.roundNumber,
        in: player.isIn(), clocks: subjectNames.map(label => clock(seats[labels.indexOf(label)])),
        counted: !!event._rest_return });
    }

    async function startCompetingClocks() {
      trial = 'competing-clocks';
      for (const player of [seats[F], seats[G]]) {
        await player.restEnd({ hp: 2 });
        await player.rest({ type: 'phase', count: 1 });
      }
      seats[G].insertPhase('G-priority-extra');
      competingClocks = true;
    }

    const observer = 'reference_rest_observer';
    lib.skill[observer] = {
      trigger: { global: [...lifecycleTimings, ...phaseTimings] }, firstDo: true, forced: true, silent: true,
      forceDie: true, forceOut: true,
      filter: (event, player) => !!seats && player === seats[A],
      async content(event, trigger) {
        try {
          const timing = event.triggername, player = trigger.player;
          if (watched(trigger, timing)) {
            observe('callback', timing, trigger);
            if (timing === cancel) trigger.cancel();
          }
          if (timing === 'phaseUseBegin' && !protocolDone && player === seats[A]) {
            await directTrials();
            trigger.finish();
          }
          if (timing === 'phaseBeforeStart' && protocolDone) {
            trigger.phaseList = ['phaseZhunbei'];
            turnState(timing, trigger);
            if (player === seats[E] && realCards && !trigger.skill && ++eNaturalTurns === 2) {
              capture('persistent-zero-unchanged', seats[F]);
              capture('persistent-negative-unchanged', seats[G]);
              capture('real-returned', player, { remainingAttendants: player.getStorage('mbdanggu').length,
                dead: game.dead.map(seat), gameOver: !!_status.over });
              // The skill's returning turn would end with a different death
              // branch. The remaining probe uses only the generic countdown.
              player.removeSkill('mbmowang');
              await startCompetingClocks();
            }
            if (player === seats[F] && competingClocks && !trigger.skill) {
              active = false;
              window.__referenceResult = { seats: labels, clockSeats: subjectNames, eventColumns, stateColumns,
                events, stages, turns };
              game.pause();
            }
          }
          if (timing === 'phaseBegin' && player === seats[C] && !trigger.skill && !roundClockStarted) {
            roundClockStarted = true;
            capture('phase-clock-returned', seats[B]);
            trial = 'round-clock';
            setVitals(seats[D]);
            await seats[D].rest({ type: 'round', count: 2 });
            capture('round-clock', seats[D]);
            seats[D].insertPhase('D-extra');
          }
          if (timing === 'phaseBegin' && player === seats[D] && !trigger.skill && ++dNaturalTurns === 2) {
            capture('round-clock-returned', seats[D]);
            trial = 'mbmowang';
            const target = seats[E];
            realCards = await setupPlayer(target);
            await target.turnOver(false);
            target.addSkill('mbdanggu');
            target.addSkill('mbmowang');
            target.markAuto('mbdanggu', ['scs_zhangrang', 'scs_zhaozhong']);
            capture('real-ready', target);
          }
          if (timing === 'phaseOver' && player === seats[E] && eNaturalTurns === 1) {
            const [hand, equip] = realCards;
            capture('real-entered', player, {
              handInDiscard: ui.discardPile.contains(hand), equipInDiscard: ui.discardPile.contains(equip),
              remainingAttendants: player.getStorage('mbdanggu').length, dead: game.dead.map(seat),
            });
            trial = 'mbmowang-repeat-death';
            const death = player.die().set('includeOut', true);
            await death;
            capture('real-repeat-death', player, { cancelled: !!death._cancelled });
            trial = 'mbmowang';
          }
        } catch (error) {
          window.__referenceError = `rest observer: ${error.stack || error}`;
        }
      },
    };
    game.finishSkill(observer);
    game.addGlobalSkill(observer);

    game.globalEventHandlers.pushHandler('phaseLoop', (event, { state }) => {
      if (seats || state !== 'begin') return;
      seats = [event.player];
      while (seats.length < game.players.length) seats.push(seats.at(-1).next);
      if (new Set(seats).size !== labels.length) throw new Error('Expected eight distinct identity seats');
      if (!_status.auto) ui.click.auto('forced');
      lib.config.game_speed = 'vvfast';
      seats.forEach(player => { player.noPhaseDelay = true; });
    });

    const proto = lib.element.GameEvent.prototype, originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (watched(this, timing)) observe('request', timing, this);
      if (active && ['phaseBefore', 'phaseZhunbeiBefore', 'roundStart'].includes(timing)) {
        turnState(timing, timing === 'phaseZhunbeiBefore' ? this.getParent('phase') : this);
      }
      return originalTrigger.apply(this, arguments);
    };
  };
}

export function checkReferenceRest(result) {
  assert.deepEqual(result.seats, [...'ABCDEFGH']);
  const named = (columns, values) => Object.fromEntries(columns.map((name, index) => [name, values[index]]));
  const stages = Object.fromEntries(result.stages.map(({ name, state, detail }) => [name,
    { ...named(result.stateColumns, state), detail },
  ]));
  const request = result.events.filter(row => row[0] === 'request').map(row => named(result.eventColumns, row));
  const observed = result.events.filter(row => row[0] === 'callback');
  assert.deepEqual(observed.map(row => row.slice(1, -1)), result.events
    .filter(row => row[0] === 'request').map(row => row.slice(1, -1)));
  const timing = name => request.filter(row => row.trial === name).map(row => row.timing);
  const revival = ['reviveBefore', 'reviveBegin', 'reviveEnd', 'reviveAfter'];

  assert.deepEqual(stages.initial, { hp: 2, maxHp: 4, alive: true, out: false, in: true, clock: null,
    mark: false, undist: false, hand: 1, equip: 1, linked: true, turnedOver: true,
    inPlayers: true, inDead: false, counted: true, previous: 'A', next: 'C', previousEligibleNext: 'B',
    identityShown: false, detail: null });
  const { detail: _initialDetail, ...initial } = stages.initial;
  const out = (clock, previousEligibleNext = 'C') => ({ out: true, in: false, clock, mark: true, undist: true,
    counted: false, previousEligibleNext });
  for (const [name, changes, cancelled] of [
    ['end-without-rest', {}, false],
    ['cancel-rest', out(['phase', -1]), true],
    ['repeat-rest', out(['phase', -1]), false],
    ['cancel-revive', {}, false],
    ['zero-clock', out(['phase', 0]), false],
    ['manual-zero-hp', { hp: 1 }, false],
    ['negative-clock', { hp: 1, ...out(['phase', -2]) }, false],
    ['cancel-rest-end', { hp: 4 }, true],
    ['repeat-end', { hp: 4 }, false],
    ['phase-clock', { hp: 4, turnedOver: false, ...out(['phase', 4]) }, false],
  ]) {
    const { detail, ...state } = stages[name];
    assert.deepEqual(state, { ...initial, ...changes }, name);
    assert.deepEqual([detail.finished, detail.cancelled], [true, cancelled], name);
  }
  for (const name of ['cancel-revive', 'cancel-rest-end']) {
    assert.deepEqual([stages[name].detail.handRetained, stages[name].detail.equipRetained], [true, true]);
  }
  for (const [name, times] of Object.entries({
    'end-without-rest': [], 'repeat-rest': [], 'repeat-end': [],
    'cancel-rest': ['rest', 'restCancelled'],
    'cancel-revive': ['reviveBefore', 'reviveCancelled', 'restEnd'],
    'zero-clock': ['rest'], 'negative-clock': ['rest'],
    'manual-zero-hp': [...revival, 'restEnd'],
    'cancel-rest-end': [...revival, 'restEnd', 'restEndCancelled'],
    'phase-clock': ['rest', ...revival, 'restEnd'],
    'round-clock': ['rest', ...revival, 'restEnd'],
  })) assert.deepEqual(timing(name), times, name);
  const cancelledRevive = request.find(row => row.trial === 'cancel-revive' && row.timing === 'reviveBefore');
  assert.deepEqual([cancelledRevive.parent, cancelledRevive.data.out, cancelledRevive.data.clock,
    cancelledRevive.data.alive, cancelledRevive.data.hp, cancelledRevive.data.reviveHp],
  ['restEnd', false, null, true, 2, 3]);

  assert.deepEqual(result.clockSeats, ['B', 'D', 'E', 'F', 'G']);
  const turn = (name, round, clockSeat) => result.turns.filter(row => row.turn === name && row.round === round)
    .map(row => [row.timing, row.in, row.counted, row.clocks[result.clockSeats.indexOf(clockSeat)]]);
  const before = 'phaseBefore', after = 'phaseBeforeStart', prepare = 'phaseZhunbeiBefore';
  assert.deepEqual(turn('C-extra', 1, 'B'), [[before, true, false, ['phase', 4]],
    [after, true, true, ['phase', 3]], [prepare, true, true, ['phase', 3]]]);
  assert.deepEqual(turn('B-extra', 1, 'B'), [[before, false, false, ['phase', 3]],
    [after, false, true, ['phase', 2]]]);
  assert.deepEqual(turn('B-natural', 1, 'B'), [[before, false, false, ['phase', 2]],
    [after, false, true, ['phase', 1]]]);
  assert.deepEqual(turn('C-natural', 1, 'B'), [[before, true, false, ['phase', 1]],
    [after, true, true, null], [prepare, true, true, null]]);
  assert.deepEqual(turn('D-extra', 1, 'D'), [[before, false, false, ['round', 2]],
    [after, false, false, ['round', 2]]]);
  assert.deepEqual(turn('D-natural', 1, 'D'), [[before, false, false, ['round', 2]],
    [after, false, true, ['round', 1]]]);
  assert.deepEqual(turn('B-natural', 2, 'D'), [[before, true, false, ['round', 1]],
    [after, true, false, ['round', 1]], [prepare, true, false, ['round', 1]]]);
  assert.deepEqual(turn('D-natural', 2, 'D'), [[before, false, false, ['round', 1]],
    [after, true, true, null], [prepare, true, true, null]]);
  assert.deepEqual(result.turns.filter(row => row.timing === 'roundStart').map(row => [row.turn, row.round]),
    [['A-natural', 2], ['A-natural', 3]]);
  for (const [name, type, count] of [['zero', 'phase', 0], ['negative', 'round', -2]]) {
    assert.deepEqual(stages[`persistent-${name}-unchanged`], stages[`persistent-${name}-placed`]);
    assert.deepEqual(stages[`persistent-${name}-unchanged`].clock, [type, count]);
  }
  for (const name of ['F-natural', 'G-natural']) {
    const inactive = result.turns.filter(row => row.turn === name && row.round < 3);
    assert.deepEqual(inactive.map(row => row.timing), [before, after, before, after]);
    assert.ok(inactive.every(row => !row.in && !row.counted));
  }

  assert.deepEqual(timing('mbmowang'), ['dieBefore', 'dieBegin', 'die', 'dieEnd', 'dieAfter', 'rest', ...revival, 'restEnd']);
  const death = request.filter(row => row.trial === 'mbmowang');
  assert.deepEqual(death.slice(0, 6).map(row => [row.parent, row.data.hp, row.data.alive, row.data.out, row.data.reserveOut]), [
    ['mbmowang_die', 2, true, false, false], ['mbmowang_die', 2, true, false, true],
    ...Array.from({ length: 3 }, () => ['mbmowang_die', 0, false, false, true]),
    ['mbmowang', 0, true, true, false],
  ]);
  assert.deepEqual(stages['real-entered'], { ...stages['real-ready'], ...out(['round', 1], 'H'), hp: 0, hand: 0,
    equip: 0, linked: false, detail: { handInDiscard: true, equipInDiscard: true, remainingAttendants: 2, dead: [] } });
  assert.deepEqual(timing('mbmowang-repeat-death'), ['dieBefore', 'dieCancelled']);
  assert.deepEqual(stages['real-repeat-death'], { ...stages['real-entered'], detail: { cancelled: true } });
  assert.deepEqual(turn('E-natural', 3, 'E'), [[before, false, false, ['round', 1]], [after, true, true, null],
    [prepare, true, true, null]]);
  assert.deepEqual(stages['real-returned'], { ...stages['real-ready'], hp: 4, equip: 0, linked: false,
    detail: { remainingAttendants: 0, dead: [], gameOver: false } });
  assert.deepEqual(turn('G-priority-extra', 3, 'G'), [[before, false, false, ['phase', 1]],
    [after, true, true, null], [prepare, true, true, null]]);
  assert.deepEqual(turn('G-priority-extra', 3, 'F'), [[before, false, false, ['phase', 1]],
    [after, true, true, ['phase', 1]], [prepare, true, true, ['phase', 1]]]);
  assert.deepEqual(turn('F-natural', 3, 'F'), [[before, false, false, ['phase', 1]], [after, true, true, null]]);
}
