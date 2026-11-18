import assert from 'node:assert/strict';

export function installPhaseQueue() {
  window.__installPhaseQueue = ({ lib, game, ui, _status }) => {
    const labels = 'ABCDEFGH';
    const scheduled = new Map();
    const events = [];
    let loop;
    let seats;
    let complete = false;
    const seat = player => player ? labels[seats.indexOf(player)] || null : null;
    const name = event => event.skill || `natural-${seat(event.player)}`;
    const queue = () => loop.next.filter(event => event.name === 'phase').map(name);
    const eventColumns = ['kind', 'timing', 'event', 'seat', 'parent', 'turn', 'naturalSeat', 'currentSeat',
      'phaseNumber', 'roundNumber', 'detail'];
    const turnTimings = ['phaseBefore', 'phaseBeforeStart', 'phaseBegin', 'phaseChange', 'phaseEnd',
      'phaseAfter', 'phaseCancelled', 'phaseOver', 'roundStart', 'roundEnd'];
    const deadTurnTimings = ['phaseBeforeEnd', 'phaseBeginStart', 'phaseZhunbeiBefore', 'phaseJudgeBefore',
      'phaseDrawBefore', 'phaseUseBefore', 'phaseDiscardBefore', 'phaseJieshuBefore'];
    const timings = [...turnTimings, ...deadTurnTimings, 'die', 'dieAfter'];

    function watched(event, timing) {
      if (!seats || complete) return false;
      if (timing === 'die' || timing === 'dieAfter') return event.player === seats[4];
      if (timing === 'phaseChange') return event.skill === 'append-dead-e';
      if (deadTurnTimings.includes(timing)) {
        return event.getParent('phase', true, true)?.skill === 'append-dead-e';
      }
      return turnTimings.includes(timing);
    }

    function capture(kind, timing, event, detail) {
      const root = event.getParent('phase', true, true);
      events.push([
        kind, timing, event.name, seat(event.player), event.getParent()?.name || null, root ? name(root) : null,
        seat(loop.player), seat(_status.currentPhase), game.phaseNumber, game.roundNumber, detail,
      ]);
    }

    function observe(kind, timing, event, callback) {
      const detail = {};
      if (['phaseBefore', 'phaseBegin', 'phaseEnd', 'phaseAfter', 'phaseOver'].includes(timing)) detail.queue = queue();
      if (event.player === seats[4]) detail.deadSeatAlive = event.player.isAlive();
      if (event.name === 'phase') {
        detail.personalPhaseNumber = event.player.phaseNumber;
        detail.historyMe = !!event.player.getHistory().isMe;
        detail.historySkipped = !!event.player.getHistory().isSkipped;
      }
      if (callback) detail.callbackParent = callback.getParent()?.name || null;
      capture(kind, timing, event, detail);
    }

    function schedule(trigger, player, source, front = false) {
      const before = queue();
      const added = player.insertPhase(source, front);
      scheduled.set(source, added);
      capture('operation', 'insertPhase', trigger, {
        recipient: seat(player), source, front, before, after: queue(),
        parent: added.getParent()?.name || null, forceDie: !!added.forceDie, includeOut: !!added.includeOut,
      });
    }

    game.globalEventHandlers.pushHandler('phaseLoop', (event, { state }) => {
      if (loop || state !== 'begin') return;
      loop = event;
      seats = [event.player];
      while (seats.length < game.players.length) seats.push(seats.at(-1).next);
      if (seats.length !== 8 || new Set(seats).size !== 8) throw new Error('Expected the eight-seat identity table');
      if (!_status.auto) ui.click.auto('forced');
      lib.config.game_speed = 'vvfast';
      seats.forEach(player => { player.noPhaseDelay = true; });
    });

    game.globalEventHandlers.pushHandler('chooseToUse', (event, { state }) => {
      if (state === 'begin' && event.type === 'phase') {
        event.ai1 = () => -10;
        event.ai2 = () => -10;
      }
    });

    const observer = 'reference_phase_queue_observer';
    lib.skill[observer] = {
      trigger: { global: timings }, forced: true, silent: true, firstDo: true,
      filter: (event, player, timing) => player === game.me && watched(event, timing),
      async content(event, trigger) {
        try {
          const timing = event.triggername;
          observe('callback', timing, trigger, event);
          if (timing === 'phaseEnd' && name(trigger) === 'natural-A') {
            schedule(trigger, seats[2], 'append-c');
            schedule(trigger, seats[3], 'append-d');
            schedule(trigger, seats[4], 'append-dead-e');
            schedule(trigger, seats[1], 'front-b', true);
            await seats[4].die().set('animate', false);
            capture('operation', 'returned-from-die', trigger, { deadSeatAlive: seats[4].isAlive(), queue: queue() });
          }
          if (timing === 'phaseEnd' && name(trigger) === 'append-c') {
            schedule(trigger, seats[5], 'nested-append-f');
          }
          if (timing === 'phaseBegin' && name(trigger) === 'natural-F') {
            complete = true;
            window.__referenceResult = {
              seats: labels.split(''), browserSeat: seat(game.me), eventColumns, events,
              final: {
                activePlayers: game.players.length,
                deadSeat: seat(seats[4]), deadSeatAlive: seats[4].isAlive(), gameOver: !!_status.over,
                queuedTurns: [...scheduled].map(([source, turn]) => ({
                  source, player: seat(turn.player), started: loop.childEvents.includes(turn),
                  finished: turn.finished, stillQueued: loop.next.includes(turn),
                })),
              },
            };
            game.pause();
          }
        } catch (error) {
          window.__referenceError = `phase-queue observer: ${error.stack || error}`;
        }
      },
    };
    game.finishSkill(observer);
    game.addGlobalSkill(observer);
    const proto = lib.element.GameEvent.prototype;
    const originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (watched(this, timing)) observe('request', timing, this);
      return originalTrigger.apply(this, arguments);
    };
  };
}

export function checkPhaseQueue(result) {
  assert.deepEqual(result.seats, [...'ABCDEFGH']);
  const timeline = result.events.map(row => Object.fromEntries(result.eventColumns.map((column, index) => [
    column, row[index],
  ])));
  const requests = timeline.filter(row => row.kind === 'request');
  const callbacks = timeline.filter(row => row.kind === 'callback');
  const identity = ({ timing, event, turn, seat }) => [timing, event, turn, seat];
  assert.deepEqual(callbacks.map(identity), requests.map(identity));
  assert.ok(callbacks.every(row => row.detail.callbackParent === 'trigger'));

  const sources = ['append-c', 'append-d', 'append-dead-e', 'front-b', 'nested-append-f'];
  const recipients = ['C', 'D', 'E', 'B', 'F'];
  const insertions = timeline.filter(row => row.kind === 'operation' && row.timing === 'insertPhase');
  assert.deepEqual(insertions.map(({ detail }) => [detail.source, detail.recipient]), sources.map((source, index) => [
    source, recipients[index],
  ]));
  assert.ok(insertions.every(({ detail }) => detail.parent === 'phaseLoop' && detail.forceDie && detail.includeOut));
  assert.deepEqual(insertions.map(({ detail }) => [detail.front, detail.before, detail.after]), [
    [false, ['natural-A'], ['natural-A', ...sources.slice(0, 1)]],
    [false, ['natural-A', ...sources.slice(0, 1)], ['natural-A', ...sources.slice(0, 2)]],
    [false, ['natural-A', ...sources.slice(0, 2)], ['natural-A', ...sources.slice(0, 3)]],
    [true, ['natural-A', ...sources.slice(0, 3)], ['front-b', 'natural-A', ...sources.slice(0, 3)]],
    [false, sources.slice(0, 3), [...sources.slice(0, 3), 'nested-append-f']],
  ]);
  assert.deepEqual(result.final, {
    activePlayers: 7, deadSeat: 'E', deadSeatAlive: false, gameOver: false,
    queuedTurns: sources.map((source, index) => ({
      source, player: recipients[index], started: source !== 'front-b', finished: source !== 'front-b', stillQueued: false,
    })),
  });

  const turnStarts = requests.filter(row => row.timing === 'phaseBefore');
  assert.deepEqual(turnStarts.map(row => [row.turn, row.naturalSeat]), [
    ['natural-A', 'A'], ['append-c', 'A'], ['append-d', 'A'], ['append-dead-e', 'A'], ['nested-append-f', 'A'],
    ['natural-B', 'B'], ['natural-C', 'C'], ['natural-D', 'D'], ['natural-F', 'F'],
  ]);
  assert.deepEqual(turnStarts[1].detail.queue, ['append-c', 'append-d', 'append-dead-e']);
  const beforeLoopAdvance = requests.find(row => row.turn === 'natural-A' && row.timing === 'phaseAfter');
  assert.deepEqual(beforeLoopAdvance.detail.queue, ['front-b', 'natural-A', ...sources.slice(0, 3)]);
  assert.deepEqual(requests.filter(row => row.timing === 'phaseOver').map(row => [
    row.seat, row.currentSeat, row.phaseNumber, row.detail.queue,
  ]), [
    ['A', null, 5, []], ['B', null, 6, []], ['C', null, 7, []], ['D', null, 8, []], ['E', null, 8, []],
  ]);
  assert.deepEqual(requests.filter(row => row.timing === 'roundStart').map(row => [
    row.turn, row.phaseNumber, row.roundNumber,
  ]), [['natural-A', 1, 1]]);
  assert.equal(requests.at(-1).turn, 'natural-F');

  const death = requests.find(row => row.timing === 'dieAfter');
  assert.deepEqual([death.turn, death.seat, death.currentSeat, death.detail.deadSeatAlive], [
    'natural-A', 'E', 'A', false,
  ]);
  const dead = requests.filter(row => row.turn === 'append-dead-e');
  assert.deepEqual(dead.map(row => row.timing), [
    'phaseBefore', 'phaseBeforeStart', 'phaseBeforeEnd', 'phaseBeginStart', 'phaseBegin', 'phaseEnd', 'phaseAfter',
  ]);
  assert.ok(dead.every(row => row.detail.deadSeatAlive === false && !row.detail.historySkipped));
  assert.deepEqual(dead.map(row => [row.phaseNumber, row.currentSeat, row.detail.personalPhaseNumber, row.detail.historyMe]), [
    [3, null, 0, false], [4, null, 0, false], [4, null, 0, false], [4, 'E', 1, true],
    [4, 'E', 1, true], [4, 'E', 1, true], [4, 'E', 1, true],
  ]);
}
