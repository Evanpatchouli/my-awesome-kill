import assert from 'node:assert/strict';

export function installCounterPayment() {
  window.__runCounterPayment = async ({ lib, game, ui, get, _status, actor: A, target: B }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const D = B.next;
    const C = game.players.find(player => ![A, B, D, game.me].includes(player));
    if (!C) throw new Error('Expected a separate recipient for the physical counter');
    const seat = player => player === A ? 'A' : player === B ? 'B' : player === C ? 'C' : player === D ? 'D' : player ? 'other' : null;
    const zone = card => {
      const owner = get.owner(card);
      const position = get.position(card, true);
      return owner ? `${seat(owner)}:${position}` : position || null;
    };
    let sample;
    const events = [];
    const trials = [];
    const choice = event => event.name === 'chooseToUse' && event.type === 'wuxie' && event.getParent().name === '_wuxie';
    const selected = event => choice(event) && event.player === A && event.result?.bool && event.result.cards?.includes(sample.counter);
    const counterUse = event => event.name === 'useCard' && event.card?.name === 'wuxie' && event.getParent().name === '_wuxie';
    const payment = event => event.name === 'lose' && event.type === 'use' && counterUse(event.getParent());
    const choiceTimings = new Set(['chooseToUseBegin', 'chooseToUseAfter']);
    const useTimings = new Set(['useCardBefore', 'useCard0', 'useCard1', 'useCard2', 'useCardAfter']);
    const materialTimings = new Set(['loseBefore', 'loseAfter', 'loseCancelled', 'gainAfter', 'cardsDiscardAfter']);
    const delayedTimings = new Set(['phaseJudge', 'judgeBegin', 'judgeEnd', 'damageBegin1']);
    function capture(kind, timing, event) {
      const row = { sample: sample.name, kind, timing, event: event.name, parent: event.getParent()?.name || null,
        player: seat(event.player), counter: zone(sample.counter), delayed: zone(sample.delayed), revealed: zone(sample.revealed), hp: B.hp };
      if (choice(event)) {
        row.accepted = event.result?.bool ?? null;
        row.selectedPhysical = !!event.result?.cards?.includes(sample.counter);
      }
      if (counterUse(event) || payment(event) || materialTimings.has(timing)) {
        row.eventHasPhysical = !!event.cards?.includes(sample.counter);
      }
      if (counterUse(event)) row.cardHasPhysical = !!event.card.cards?.includes(sample.counter);
      if (payment(event)) row.usedBy = seat(event.getParent().player);
      if (event.name === '_wuxie') row.triggerTiming = event.triggername;
      events.push(row);
    }

    const proto = lib.element.GameEvent.prototype;
    const originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (sample && ((choiceTimings.has(timing) && choice(this)) || (useTimings.has(timing) && counterUse(this)) ||
          (materialTimings.has(timing) && (payment(this) || this.cards?.includes(sample.counter))) ||
          (delayedTimings.has(timing) && this.player === B))) {
        capture('request', timing, this);
      }
      return originalTrigger.apply(this, arguments);
    };
    game.globalEventHandlers.pushHandler('chooseToUse', (event, { state }) => {
      if (sample && state === 'begin' && choice(event)) {
        event.ai1 = () => !sample.selected && event.player === A ? 100 : -100;
      }
    });
    const seenContests = new WeakSet();
    game.globalEventHandlers.pushHandler('_wuxie', (event, { state }) => {
      if (sample && state === 'begin' && !seenContests.has(event)) {
        seenContests.add(event);
        capture('content', state, event);
      }
    });
    for (const name of ['wuxie', 'shandian', 'shandianCancel']) {
      game.globalEventHandlers.pushHandler(name, (event, { state }) => {
        if (sample) capture('content', state, event);
      });
    }
    const observer = 'reference_counter_payment';
    lib.skill[observer] = {
      trigger: { global: ['chooseToUseAfter', 'loseBefore'] }, firstDo: true, forced: true, silent: true,
      filter(event, player, timing) {
        if (!sample || player !== A) return false;
        if (timing === 'chooseToUseAfter') return !sample.selected && selected(event);
        return payment(event) && event.cards.includes(sample.counter);
      },
      async content(event, trigger) {
        const timing = event.triggername;
        capture('callback', timing, trigger);
        if (timing === 'chooseToUseAfter') sample.selected = true;
        if ((timing === 'chooseToUseAfter' && sample.name === 'move-after-choice') ||
            (timing === 'loseBefore' && sample.name === 'move-in-payment')) {
          await C.gain(sample.counter, A, 'give');
          capture('callback', 'after-transfer', trigger);
        } else if (timing === 'loseBefore' && sample.name === 'cancel-payment') {
          await trigger.cancel();
        }
      },
    };
    game.finishSkill(observer);
    game.addGlobalSkill(observer);

    try {
      for (const name of ['move-after-choice', 'cancel-payment', 'move-in-payment']) {
        for (const player of game.players) {
          const held = player.getCards('hej');
          if (held.length) await player.discard(held);
        }
        B.hp = B.maxHp = 8;
        B.update();
        const delayed = game.createCard2('shandian', 'spade', 1);
        const revealed = game.createCard2('sha', 'spade', 8);
        const counter = game.createCard2('wuxie', 'spade', 11);
        await A.gain(counter);
        await B.addJudge(delayed);
        ui.cardPile.prepend(revealed);
        const before = { hp: B.hp, counter: zone(counter), delayed: zone(delayed), revealed: zone(revealed) };
        sample = { name, counter, delayed, revealed, selected: false };
        await B.phaseJudge();
        const after = { hp: B.hp, counter: zone(counter), delayed: zone(delayed), revealed: zone(revealed) };
        trials.push({ name, selected: sample.selected, before, after });
        sample = undefined;
        window.__referenceStages.push(`${name}: resolved`);
      }
      return { players: game.players.length, nextJudgmentSeat: seat(D), trials, events };
    } finally {
      proto.trigger = originalTrigger;
      game.removeGlobalSkill(observer);
    }
  };
}

export function checkCounterPayment(result) {
  assert.equal(result.players, 8);
  assert.deepEqual(result.trials.map(trial => trial.name), ['move-after-choice', 'cancel-payment', 'move-in-payment']);
  const requests = trial => result.events.filter(row => row.sample === trial.name && row.kind === 'request');
  const [afterChoice, cancelled, duringPayment] = result.trials;
  for (const [trial, materialAtUse, materialAtEnd] of [[afterChoice, 'o', 'd'], [cancelled, 'A:h', 'A:h'], [duringPayment, 'C:h', 'C:h']]) {
    assert.equal(trial.selected, true);
    assert.deepEqual(trial.before, { hp: 8, counter: 'A:h', delayed: 'B:j', revealed: 'c' });
    assert.deepEqual(trial.after, { hp: 8, counter: materialAtEnd, delayed: `${result.nextJudgmentSeat}:j`, revealed: 'c' });
    const all = result.events.filter(row => row.sample === trial.name);
    const rows = requests(trial);
    const choice = rows.findIndex(row => row.timing === 'chooseToUseAfter' && row.accepted);
    const use = rows.findIndex(row => row.timing === 'useCardBefore');
    assert.ok(choice >= 0 && use > choice);
    assert.deepEqual([rows[choice].player, rows[choice].parent, rows[choice].counter, rows[choice].selectedPhysical], ['A', '_wuxie', 'A:h', true]);
    assert.deepEqual([rows[use].player, rows[use].parent, rows[use].eventHasPhysical, rows[use].cardHasPhysical], ['A', '_wuxie', true, true]);
    const useTimings = rows.filter(row => row.event === 'useCard' && row.timing !== 'useCardBefore');
    assert.deepEqual(useTimings.map(row => row.timing), ['useCard0', 'useCard1', 'useCard2', 'useCardAfter']);
    for (const row of useTimings) {
      assert.equal(row.counter, materialAtUse);
      assert.equal(row.player, 'A');
      assert.equal(row.eventHasPhysical, true);
      assert.equal(row.cardHasPhysical, true);
    }
    assert.ok(all.some(row => row.event === '_wuxie' && row.triggerTiming === 'phaseJudge'));
    assert.ok(all.some(row => row.event === 'wuxie' && row.timing === 'end'));
    assert.ok(all.some(row => row.event === 'shandianCancel' && row.timing === 'end'));
    assert.ok(!all.some(row => ['judge', 'damage', 'shandian'].includes(row.event)));
  }
  const payment = trial => requests(trial).filter(row => row.event === 'lose' && row.parent === 'useCard')
    .map(row => [row.timing, row.player, row.usedBy, row.counter, row.eventHasPhysical]);
  assert.deepEqual(payment(afterChoice), [
    ['loseBefore', 'C', 'A', 'C:h', true], ['loseAfter', 'C', 'A', 'o', true],
  ]);
  assert.deepEqual(payment(cancelled), [
    ['loseBefore', 'A', 'A', 'A:h', true], ['loseCancelled', 'A', 'A', 'A:h', true],
  ]);
  assert.deepEqual(payment(duringPayment), [
    ['loseBefore', 'A', 'A', 'A:h', true], ['loseAfter', 'A', 'A', 'C:h', false],
  ]);
  assert.deepEqual(requests(afterChoice).filter(row => row.timing === 'cardsDiscardAfter').map(row => row.counter), ['d']);
  assert.ok(![...requests(cancelled), ...requests(duringPayment)].some(row => row.timing === 'cardsDiscardAfter'));
  const transfer = trial => result.events.find(row => row.sample === trial.name && row.timing === 'after-transfer');
  assert.deepEqual([transfer(afterChoice).event, transfer(afterChoice).counter], ['chooseToUse', 'C:h']);
  assert.deepEqual([transfer(duringPayment).event, transfer(duringPayment).counter], ['lose', 'C:h']);
}
