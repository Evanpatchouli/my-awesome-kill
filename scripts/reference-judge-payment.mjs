import assert from 'node:assert/strict';

export function installJudgePayment() {
  window.__runJudgePayment = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1], C = seats[2];
    if (B === game.zhu) throw new Error('Expected a non-lord target');
    if (B === game.me) { game.swapPlayer(A); B.setIdentity('cai'); }
    const seat = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    const names = ['useCard', 'lose', 'gain', 'discard', 'equip', 'addToExpansion', 'addJudge', 'cardsDiscard'];
    const timings = [...new Set([...names.flatMap(name => ['', 'Before', 'Begin', 'End', 'After', 'Cancelled'].map(x => name + x)),
      'useCard1', 'useCard2', 'useCardToPlayer', 'useCardToTarget', 'useCardToPlayered', 'useCardToTargeted'])];
    const events = [], trials = [];
    let trial = null, material = null, mode = null, handled = false, cleanupHandled = false, moving = null;
    const zone = card => { const position = get.position(card, true), owner = get.owner(card);
      return owner && position ? `${seat(owner)}:${position}` : position ?? null; };
    const state = () => ({ zone: zone(material), knownC: material.isKnownBy(C),
      judges: B.getVCards('j').map(card => card.name), inOrdering: ui.ordering.contains(material),
      disabled: B.isDisabledJudge(), owner: seat(get.owner(material)), handC: C.countCards('h'),
      inDiscard: ui.discardPile.contains(material) });
    function record(kind, timing, event) {
      if (!trial || !timings.includes(timing)) return;
      events.push([trial, kind, timing, event.name, event.getParent()?.name ?? null,
        seat(event.player), seat(event.source), seat(event.target), !!event.finished, !!event._cancelled,
        event.cards?.length ?? null, event.cards?.some(card => card === material || card.cards?.includes(material)) ?? null, state()]);
    }
    const observer = 'reference_judge_payment_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true,
      filter: () => !!trial,
      async content(event, trigger) {
        record('callback', event.triggername, trigger);
        if (!handled && moving && event.triggername === moving.timing && trigger.player === B &&
          trigger.getParent('disableJudge', true)) {
          handled = true;
          if (moving.destination === 'h') await B.gain(material);
          else if (moving.destination === 'e') { await B.gain(material); await B.equip(material); }
          else await B.addToExpansion(material, B, 'give');
        }
        if (!handled && mode && mode !== 'normal' && event.triggername === 'loseBefore' &&
          !mode.startsWith('block') && trigger.player === A && trigger.getParent()?.name === 'useCard') {
          handled = true;
          if (mode === 'cancel') trigger.cancel();
          else if (mode === 'transfer') await C.gain(material);
          else await A.discard(material);
        }
        if (!handled && mode?.startsWith('block') && event.triggername === 'useCardToTargeted' && trigger.target === B) {
          handled = true;
          await B.disableJudge();
        }
        if (!cleanupHandled && mode?.startsWith('block-') && event.triggername === 'cardsDiscardBefore' &&
          trigger.getParent()?.name === 'orderingDiscard' && trigger.cards?.includes(material)) {
          cleanupHandled = true;
          if (mode === 'block-cancel') trigger.cancel();
          else await C.gain(material);
        }
      } };
    game.finishSkill(observer); A.addSkill(observer);
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return original.apply(this, arguments); };
    const reset = async () => {
      trial = null;
      if (B.isDisabledJudge()) await B.enableJudge();
      for (const player of seats) {
        const held = player.getCards('hej');
        if (held.length) await player.discard(held);
      }
    };
    try {
      lib.skill.guose = (await import('/character/standard/skill.js')).default.guose;
      game.finishSkill('guose'); A.addSkill('guose');
      for (const kind of ['lebu', 'guose']) {
        for (const effect of ['normal', 'cancel', 'transfer', 'discard', 'block',
          ...(kind === 'lebu' ? ['block-cancel', 'block-transfer'] : [])]) {
          await reset();
          material = game.createCard2(kind === 'lebu' ? 'lebu' : 'sha', 'diamond', 6);
          await A.gain(material);
          trial = `${kind}:${effect}`; mode = effect; handled = false; cleanupHandled = false;
          const before = state();
          const use = kind === 'lebu' ? A.useCard(material, B, false)
            : A.useCard({ name: 'lebu', isCard: true }, [material], B, false);
          if (kind === 'guose') use.skill = 'guose';
          await use;
          trials.push({ name: trial, handled, cleanupHandled, before, after: state(), finished: !!use.finished, cancelled: !!use._cancelled });
        }
      }
      mode = null;
      const movements = [];
      for (const timing of ['discardBefore', 'loseBefore']) {
        for (const destination of ['h', 'e', 'x']) {
          await reset();
          material = game.createCard2(destination === 'e' ? 'qinggang' : 'lebu', 'diamond', 6);
          if (destination === 'e') await B.addJudge({ name: 'lebu' }, [material]);
          else await B.addJudge(material);
          trial = `move:${timing}:${destination}`; handled = false;
          moving = { timing, destination };
          const before = state();
          await B.disableJudge();
          const after = state();
          movements.push({ name: trial, handled, before, after });
          moving = null;
        }
      }
      const placements = [];
      for (const origin of ['discard', 'deck', 'special', 'expansion']) {
        await reset();
        material = game.createCard2('lebu', 'heart', 7);
        if (origin === 'discard') ui.discardPile.append(material);
        else if (origin === 'deck') ui.cardPile.prepend(material);
        else if (origin === 'special') ui.special.append(material);
        else { await A.gain(material); await A.addToExpansion(material, A, 'give'); }
        trial = `origin:${origin}`;
        const before = state(), allowed = B.canAddJudge(material);
        await B.addJudge(material);
        placements.push({ origin, before, allowed, after: state() });
      }
      await reset();
      await B.die().set('animate', false);
      material = game.createCard2('lebu', 'heart', 7);
      await A.gain(material);
      trial = 'dead-target';
      const dead = { alive: B.isAlive(), out: B.isOut(), in: B.isIn(), canAdd: B.canAddJudge(material), before: state() };
      await B.addJudge(material);
      dead.after = state();
      return { columns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'target',
        'finished', 'cancelled', 'cardCount', 'containsMaterial', 'state'], events, trials, movements, placements, dead };
    } finally { proto.trigger = original; A.removeSkill(observer); }
  };
}

export function checkJudgePayment(result) {
  assert.equal(result.trials.length, 12);
  for (const kind of ['lebu', 'guose']) {
    for (const [effect, position, judges] of [['normal', kind === 'lebu' ? 'B:j' : 'j', ['lebu']],
      ['cancel', 'A:h', []], ['transfer', 'C:h', []], ['discard', 'd', []], ['block', 'd', []]]) {
      const trial = result.trials.find(item => item.name === `${kind}:${effect}`);
      assert.deepEqual([trial.after.zone, trial.after.judges, trial.finished, trial.cancelled], [position, judges, true, false], trial.name);
      const rows = result.events.filter(row => row[0] === trial.name && row[1] === 'request');
      const names = rows.map(row => row[2]);
      for (const timing of ['useCardBefore', 'useCardBegin', 'useCard1', 'useCard2', 'useCard', 'useCardToPlayer',
        'useCardToTarget', 'useCardToPlayered', 'useCardToTargeted', 'useCardEnd', 'useCardAfter']) {
        assert.ok(names.includes(timing), `${trial.name}: ${timing}`);
      }
      assert.equal(names.includes('addJudgeBefore'), effect === 'normal');
      if (effect === 'block') assert.equal(rows.find(row => row[2] === 'useCardAfter')[12].zone, 'o');
    }
  }
  for (const [effect, position, hand, discarded] of [['block-cancel', 'o', 0, false], ['block-transfer', 'd', 0, true]]) {
    const trial = result.trials.find(item => item.name === `lebu:${effect}`);
    assert.deepEqual([trial.handled, trial.cleanupHandled, trial.after.zone, trial.after.handC, trial.after.inDiscard],
      [true, true, position, hand, discarded]);
    const rows = result.events.filter(row => row[0] === trial.name && row[1] === 'request');
    assert.deepEqual([rows.find(row => row[2] === 'useCardAfter')[12].zone,
      rows.find(row => row[2] === 'cardsDiscardBefore')[12].zone], ['o', 'o']);
    assert.equal(rows.some(row => row[2] === 'cardsDiscardAfter'), effect === 'block-transfer');
    assert.equal(rows.some(row => row[2] === 'cardsDiscardCancelled'), effect === 'block-cancel');
  }
  assert.equal(result.movements.length, 6);
  for (const trial of result.movements) {
    assert.deepEqual([trial.handled, trial.after.zone, trial.after.disabled, trial.after.judges], [true, 'd', true, []], trial.name);
  }
  assert.deepEqual(result.placements.map(item => [item.origin, item.before.zone, item.allowed, item.after.zone]),
    [['discard', 'd', true, 'B:j'], ['deck', 'c', true, 'B:j'], ['special', 's', true, 'B:j'],
      ['expansion', 'A:x', true, 'B:j']]);
  assert.deepEqual([result.dead.alive, result.dead.out, result.dead.in, result.dead.canAdd,
    result.dead.before.zone, result.dead.after.zone, result.dead.after.judges], [false, false, false, true, 'A:h', 'A:h', []]);
}
