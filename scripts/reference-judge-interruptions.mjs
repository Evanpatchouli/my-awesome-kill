import assert from 'node:assert/strict';

export function installJudgeInterruptions() {
  window.__runJudgeInterruptions = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1], seat = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    const names = ['useCard', 'useCardToPlayer', 'useCardToPlayered', 'useCardToTarget', 'useCardToTargeted',
      'addJudge', 'disableJudge', 'enableJudge', 'discard', 'lose', 'phaseJudge', 'judge', 'cardsDiscard'];
    const timings = [...new Set(names.flatMap(name => ['', 'Before', 'Begin', 'End', 'After', 'Cancelled'].map(x => name + x)))];
    const cards = new Map(), events = [], trials = [];
    let trial = null, interrupt = null, performed = false, transfer = null;
    const make = (id, name, suit, number) => { const card = game.createCard2(name, suit, number); cards.set(card, id); return card; };
    const zone = card => { const pos = get.position(card, true), owner = get.owner(card); return owner && pos ? `${seat(owner)}:${pos}` : pos ?? null; };
    const state = () => ({ disabled: B.isDisabledJudge(),
      placeholder: [...B.node.judges.childNodes].filter(card => card.classList.contains('feichu')).length,
      judges: B.getVCards('j').map(card => card.name),
      positions: Object.fromEntries([...cards].map(([card, id]) => [id, zone(card)])) });
    function record(kind, timing, event) {
      if (!trial || !timings.includes(timing)) return;
      events.push([trial, kind, timing, event.name, event.getParent()?.name ?? null, seat(event.player),
        seat(event.source), seat(event.target), !!event.finished, !!event._cancelled, state()]);
    }
    const observer = 'reference_judge_interruptions_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true, forceOut: true,
      filter: () => !!trial,
      async content(event, trigger) {
        record('callback', event.triggername, trigger);
        if (transfer && !performed && event.triggername === transfer.timing &&
          (transfer.timing === 'addJudgeBefore' ? trigger.player === B : trigger.getParent()?.name === 'addJudge')) {
          performed = true;
          if (transfer.mode === 'redirect-add') await seats[2].gain(transfer.card);
          else if (transfer.mode === 'redirect-lose') await A.discard(transfer.card);
          else trigger.cancel();
        }
        if (!performed && interrupt === event.triggername &&
          (event.triggername.startsWith('useCardTo') ? trigger.target === B : trigger.player === B)) {
          performed = true;
          if (interrupt === 'disableJudgeBefore' || interrupt === 'discardBefore' || interrupt === 'loseBefore') {
            trigger.cancel();
          } else if (interrupt === 'discard') {
            await B.enableJudge();
          } else {
            await B.disableJudge();
          }
        }
      } };
    game.finishSkill(observer); A.addSkill(observer);
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return original.apply(this, arguments); };
    const reset = async () => {
      trial = interrupt = null;
      if (B.isDisabledJudge()) await B.enableJudge();
      for (const player of seats) {
        const held = player.getCards('hej');
        if (held.length) await player.discard(held);
      }
      cards.clear();
    };
    try {
      lib.skill.guose = (await import('/character/standard/skill.js')).default.guose;
      game.finishSkill('guose');
      A.addSkill('guose');
      for (const kind of ['lebu', 'guose']) {
        for (const timing of ['useCardToPlayer', 'useCardToTarget', 'useCardToPlayered', 'useCardToTargeted',
          'addJudgeBefore', 'addJudgeBegin']) {
          await reset();
          const material = make('material', kind === 'lebu' ? 'lebu' : 'sha', 'diamond', 6);
          await A.gain(material);
          trial = `${kind}:${timing}`; interrupt = timing; performed = false;
          const use = kind === 'lebu' ? A.useCard(material, B, false)
            : A.useCard({ name: 'lebu', isCard: true }, [material], B, false);
          if (kind === 'guose') use.skill = 'guose';
          await use;
          trials.push({ name: trial, performed, state: state() });
        }
      }
      await reset();
      await B.disableJudge();
      const direct = make('direct', 'lebu', 'heart', 6);
      await A.gain(direct);
      trial = 'direct-disabled';
      const directCanAdd = B.canAddJudge(direct);
      await B.addJudge(direct);
      trials.push({ name: trial, canAdd: directCanAdd, state: state() });

      for (const cancellation of ['discardBefore', 'loseBefore']) {
        await reset();
        const delayed = make('delayed', 'lebu', 'club', 6);
        const reveal = make('reveal', 'sha', 'heart', 9);
        await B.addJudge(delayed);
        trial = `retained:${cancellation}`; interrupt = cancellation; performed = false;
        await B.disableJudge();
        const beforeJudge = state();
        interrupt = null;
        ui.cardPile.prepend(reveal);
        await B.phaseJudge();
        trials.push({ name: trial, performed, beforeJudge, afterJudge: state(), skipped: B.skipList.slice() });
        B.skipList.length = 0;
      }
      await reset();
      const lightning = make('lightning', 'shandian', 'spade', 1);
      for (const player of seats.slice(1)) await player.disableJudge();
      await A.addJudge(lightning);
      await A.lose(lightning, ui.ordering);
      trial = 'lightning-self';
      const candidate = seats.map(player => player.canAddJudge('shandian'));
      await A.addJudgeNext(lightning);
      trials.push({ name: trial, candidate, zone: zone(lightning), owner: seat(get.owner(lightning)) });
      trial = null;
      for (const player of seats.slice(1)) await player.enableJudge();

      for (const mode of ['disableJudgeBefore', 'discard']) {
        await reset();
        const delayed = make('delayed', 'lebu', 'club', 6);
        await B.addJudge(delayed);
        trial = `outer:${mode}`; interrupt = mode; performed = false;
        await B.disableJudge();
        trials.push({ name: trial, performed, state: state() });
      }
      for (const [form, mode] of ['physical', 'virtual'].flatMap(form =>
        ['redirect-add', 'redirect-lose', 'cancel-lose'].map(mode => [form, mode]))) {
        await reset();
        const material = make('transfer', form === 'physical' ? 'lebu' : 'sha', 'heart', 7);
        await A.gain(material);
        const knownBefore = material.isKnownBy(seats[2]);
        trial = `transfer:${form}:${mode}`; performed = false;
        transfer = { mode, card: material, timing: mode === 'redirect-add' ? 'addJudgeBefore' : 'loseBefore' };
        if (form === 'physical') await B.addJudge(material);
        else await B.addJudge({ name: 'lebu' }, [material]);
        transfer = null;
        trials.push({ name: trial, performed, knownBefore, knownAfter: material.isKnownBy(seats[2]), state: state() });
      }
      return { columns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'target',
        'finished', 'cancelled', 'state'], events, trials };
    } finally { proto.trigger = original; A.removeSkill(observer); }
  };
}

export function checkJudgeInterruptions(result) {
  assert.equal(result.trials.length, 24);
  for (const kind of ['lebu', 'guose']) {
    for (const timing of ['useCardToPlayer', 'useCardToTarget', 'useCardToPlayered', 'useCardToTargeted',
      'addJudgeBefore', 'addJudgeBegin']) {
      const trial = result.trials.find(item => item.name === `${kind}:${timing}`);
      assert.equal(trial.performed, true, trial.name);
      const placed = timing.startsWith('addJudge');
      assert.deepEqual([trial.state.disabled, trial.state.placeholder, trial.state.judges],
        [true, 1, placed ? ['lebu'] : []], trial.name);
      assert.ok(placed ? ['B:j', 'j'].includes(trial.state.positions.material) : trial.state.positions.material === 'd', trial.name);
    }
  }
  const getTrial = name => result.trials.find(item => item.name === name);
  assert.deepEqual([getTrial('direct-disabled').canAdd, getTrial('direct-disabled').state.positions.direct], [false, 'B:j']);
  for (const when of ['discardBefore', 'loseBefore']) {
    const trial = getTrial(`retained:${when}`);
    assert.equal(trial.performed, true);
    assert.deepEqual([trial.beforeJudge.disabled, trial.beforeJudge.judges, trial.beforeJudge.positions.delayed], [true, ['lebu'], 'B:j']);
    assert.deepEqual([trial.afterJudge.disabled, trial.afterJudge.judges, trial.afterJudge.positions.delayed,
      trial.afterJudge.positions.reveal], [true, [], 'd', 'd']);
  }
  assert.deepEqual([getTrial('lightning-self').candidate, getTrial('lightning-self').zone],
    [[true, false, false, false, false, false, false, false], 'A:j']);
  assert.deepEqual([getTrial('outer:disableJudgeBefore').performed, getTrial('outer:disableJudgeBefore').state.disabled,
    getTrial('outer:disableJudgeBefore').state.placeholder, getTrial('outer:disableJudgeBefore').state.positions.delayed],
  [true, false, 0, 'B:j']);
  assert.deepEqual([getTrial('outer:discard').performed, getTrial('outer:discard').state.disabled,
    getTrial('outer:discard').state.placeholder, getTrial('outer:discard').state.positions.delayed], [true, true, 1, 'd']);
  const nested = result.events.filter(row => row[0] === 'outer:discard' && row[1] === 'request');
  const at = timing => nested.find(row => row[2] === timing)[10];
  assert.deepEqual([at('discard').disabled, at('discard').placeholder, at('enableJudgeEnd').disabled,
    at('discardAfter').disabled, at('disableJudgeEnd').disabled, at('disableJudgeEnd').placeholder],
  [true, 0, false, false, true, 1]);
  for (const [mode, position, judges] of [['redirect-add', 'B:j', ['lebu']],
    ['redirect-lose', 'B:j', ['lebu']], ['cancel-lose', 'A:h', []]]) {
    const trial = getTrial(`transfer:physical:${mode}`);
    assert.deepEqual([trial.performed, trial.knownBefore, trial.knownAfter, trial.state.positions.transfer, trial.state.judges],
      [true, false, true, position, judges], mode);
    const virtual = getTrial(`transfer:virtual:${mode}`);
    assert.deepEqual([virtual.performed, virtual.knownBefore, virtual.knownAfter, virtual.state.positions.transfer, virtual.state.judges],
      [true, false, true, mode === 'cancel-lose' ? 'A:h' : 'j', judges], `virtual:${mode}`);
  }
}
