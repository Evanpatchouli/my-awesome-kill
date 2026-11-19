import assert from 'node:assert/strict';

export function installJudgeArea() {
  window.__runJudgeArea = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced');
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const seat = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    const B = seats[1], C = seats[2];
    const timings = ['disableJudge', 'enableJudge', 'discard', 'lose', 'addJudge', 'useSkill', 'draw'].flatMap(name =>
      ['', 'Before', 'Begin', 'End', 'After', 'Cancelled'].map(suffix => name + suffix));
    const events = [], trials = [];
    let current = null, cancelAt = null;
    const tracked = new Map();
    const zone = card => {
      const position = get.position(card, true), owner = get.owner(card);
      return owner && position ? `${seat(owner)}:${position}` : position ?? null;
    };
    const cardNames = cards => cards.flatMap(card => tracked.has(card) ? [tracked.get(card)]
      : (card.cards || []).filter(item => tracked.has(item)).map(item => tracked.get(item)));
    const make = (id, name, suit, number) => {
      const card = game.createCard2(name, suit, number);
      tracked.set(card, id);
      return card;
    };
    const snapshot = name => ({ name, disabled: B.isDisabledJudge(),
      placeholder: [...B.node.judges.childNodes].filter(card => card.classList.contains('feichu')).length,
      judge: B.getCards('j').map(card => tracked.get(card) ?? card.name),
      virtual: B.getVCards('j').map(card => card.name),
      hand: B.getCards('h').filter(card => tracked.has(card)).map(card => tracked.get(card)),
      equip: B.getCards('e').filter(card => tracked.has(card)).map(card => tracked.get(card)),
      cards: Object.fromEntries([...tracked].map(([card, id]) => [id, zone(card)])),
      canAdd: ['lebu', 'shandian', 'bingliang'].map(card => B.canAddJudge(card)),
      canUseLebu: A.canUse({ name: 'lebu' }, B, false) });
    function record(kind, timing, event) {
      if (!current || !timings.includes(timing)) return;
      events.push([current, kind, timing, event.name, event.getParent()?.name ?? null,
        seat(event.player), seat(event.source), cardNames(event.cards || []),
        B.isDisabledJudge(), [...B.node.judges.childNodes].filter(card => card.classList.contains('feichu')).length,
        B.getCards('j').map(card => tracked.get(card) ?? card.name),
        Object.fromEntries([...tracked].map(([card, id]) => [id, zone(card)])),
        !!event.finished, !!event._cancelled]);
    }
    const observer = 'reference_judge_area_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true,
      filter: () => !!current,
      async content(event, trigger) {
        record('callback', event.triggername, trigger);
        if (event.triggername === cancelAt && trigger.getParent('disableJudge', true)) trigger.cancel();
      } };
    game.finishSkill(observer);
    A.addSkill(observer);
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return original.apply(this, arguments); };
    try {
      lib.skill.guose = (await import('/character/standard/skill.js')).default.guose;
      game.finishSkill('guose');
      A.addSkill('guose');
      for (const player of game.players) {
        const held = player.getCards('h');
        if (held.length) await player.discard(held);
      }
      for (const [name, cancellation, populated] of [
        ['ordinary', null, true], ['cancel-discard', 'discardBefore', true],
        ['cancel-lose', 'loseBefore', true], ['empty', null, false],
      ]) {
        current = null;
        if (B.isDisabledJudge()) await B.enableJudge();
        for (const player of [A, B, C]) {
          const cards = player.getCards('hej');
          if (cards.length) await player.discard(cards);
        }
        tracked.clear();
        const hand = make('hand', 'sha', 'spade', 7);
        const equip = make('equip', 'zhuge', 'club', 1);
        await B.gain(hand);
        await B.equip(equip);
        if (populated) {
          const material = make('guose-material', 'sha', 'diamond', 6);
          const lightning = make('lightning', 'shandian', 'spade', 1);
          await A.gain(material);
          const use = A.useCard({ name: 'lebu', isCard: true }, [material], B, false);
          use.skill = 'guose';
          await use;
          await B.addJudge(lightning);
        }
        const stages = [snapshot('before')];
        current = name; cancelAt = cancellation;
        await B.disableJudge();
        stages.push(snapshot('disabled'));
        cancelAt = null;
        await B.disableJudge();
        stages.push(snapshot('disabled-again'));
        await B.enableJudge();
        stages.push(snapshot('enabled'));
        await B.enableJudge();
        stages.push(snapshot('enabled-again'));
        current = null;
        trials.push({ name, stages });
      }
      // The ordinary lightning transfer consults judge-area eligibility and
      // skips B while disabled, then reaches B once restored.
      current = null;
      for (const player of [A, B, C]) {
        const cards = player.getCards('hej');
        if (cards.length) await player.discard(cards);
      }
      tracked.clear();
      const blocked = make('lightning-blocked', 'shandian', 'spade', 1);
      const allowed = make('lightning-allowed', 'shandian', 'club', 1);
      const lebu = make('lebu-used', 'lebu', 'heart', 6);
      await B.disableJudge();
      await A.addJudge(blocked);
      current = 'routing-disabled';
      const disabledCanUse = A.canUse(lebu, B, false);
      await A.addJudgeNext(blocked);
      const blockedDestination = zone(blocked);
      current = null;
      await B.enableJudge();
      await A.addJudge(allowed);
      current = 'routing-enabled';
      const enabledCanUse = A.canUse(lebu, B, false);
      await A.addJudgeNext(allowed);
      await A.gain(lebu);
      await A.useCard(lebu, B, false);
      const routing = { disabledCanUse, blockedDestination, enabledCanUse,
        allowedDestination: zone(allowed), lebuDestination: zone(lebu), afterUse: snapshot('after-use') };
      current = null;
      // Choose only the judgment-area branch of the actual skill backup; this
      // does not exercise the equipment choices or interactive button dialog.
      lib.skill.midu = (await import('/character/huicui/skill.js')).default.midu;
      game.finishSkill('midu');
      B.addSkill('midu');
      const midu = {};
      const handBefore = A.countCards('h');
      lib.skill.midu_backup = lib.skill.midu.chooseButton.backup([[-1, true]], B);
      game.finishSkill('midu_backup');
      current = 'midu-disable';
      await B.useSkill({ skill: 'midu_backup', cards: [], targets: [A] });
      midu.disabled = snapshot('midu-disabled');
      midu.drawn = A.countCards('h') - handBefore;
      current = null;
      lib.skill.midu_backup = lib.skill.midu.chooseButton.backup([[-1, false]], B);
      game.finishSkill('midu_backup');
      current = 'midu-enable';
      await B.useSkill({ skill: 'midu_backup', cards: [], targets: [] });
      midu.enabled = snapshot('midu-enabled');
      midu.rehuomo = B.hasSkill('rehuomo');
      current = null;
      return { eventColumns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'cards',
        'disabled', 'placeholder', 'judge', 'zones', 'finished', 'cancelled'], events, trials, routing, midu };
    } finally {
      proto.trigger = original;
      A.removeSkill(observer);
    }
  };
}

export function checkJudgeArea(result) {
  assert.deepEqual(result.trials.map(trial => trial.name), ['ordinary', 'cancel-discard', 'cancel-lose', 'empty']);
  for (const trial of result.trials) {
    const [before, disabled, repeated, enabled, enabledAgain] = trial.stages;
    const populated = trial.name !== 'empty';
    assert.deepEqual([before.disabled, before.placeholder, before.hand, before.equip], [false, 0, ['hand'], ['equip']]);
    assert.deepEqual(before.canAdd, populated ? [false, false, true] : [true, true, true]);
    assert.deepEqual([disabled.disabled, disabled.placeholder, disabled.canAdd], [true, 1, [false, false, false]]);
    assert.deepEqual([repeated.disabled, repeated.placeholder, repeated.judge], [true, 1, []]);
    assert.deepEqual([enabled.disabled, enabled.placeholder, enabled.judge, enabled.canAdd],
      [false, 0, [], [true, true, true]]);
    assert.deepEqual({ ...enabledAgain, name: 'enabled' }, enabled);
    if (populated) {
      assert.deepEqual([before.virtual, before.cards['guose-material'], before.cards.lightning],
        [['lebu', 'shandian'], 'B:j', 'B:j']);
      const cancelled = trial.name.startsWith('cancel-');
      assert.deepEqual([disabled.cards['guose-material'], disabled.cards.lightning],
        cancelled ? ['B:j', 'B:j'] : ['d', 'd']);
      assert.deepEqual([repeated.cards['guose-material'], repeated.cards.lightning,
        enabled.cards['guose-material'], enabled.cards.lightning], ['d', 'd', 'd', 'd']);
    }
  }
  const rows = result.events.map(row => Object.fromEntries(result.eventColumns.map((column, index) => [column, row[index]])));
  const ordinary = rows.filter(row => row.trial === 'ordinary' && row.kind === 'request');
  const find = timing => ordinary.find(row => row.timing === timing);
  assert.deepEqual([find('disableJudgeBefore').player, find('disableJudgeBefore').source,
    find('disableJudgeBefore').disabled, find('disableJudgeBefore').placeholder], ['B', 'A', false, 0]);
  assert.deepEqual([find('discardBefore').parent, find('discardBefore').disabled, find('discardBefore').placeholder,
    find('discardBefore').cards], ['disableJudge', true, 0, ['lightning', 'guose-material']]);
  assert.deepEqual([find('loseBefore').parent, find('loseBefore').cards], ['discard', ['lightning', 'guose-material']]);
  assert.deepEqual([find('loseEnd').zones.lightning, find('loseEnd').zones['guose-material'], find('loseEnd').placeholder],
    ['d', 'd', 0]);
  assert.deepEqual([find('disableJudgeEnd').disabled, find('disableJudgeEnd').placeholder], [true, 1]);
  for (const [name, cancelled] of [['cancel-discard', 'discardCancelled'], ['cancel-lose', 'loseCancelled']]) {
    const timings = rows.filter(row => row.trial === name && row.kind === 'request').map(row => row.timing);
    assert.ok(timings.includes(cancelled) && timings.includes('disableJudgeEnd'));
  }
  assert.deepEqual([result.routing.disabledCanUse, result.routing.blockedDestination, result.routing.enabledCanUse,
    result.routing.allowedDestination, result.routing.lebuDestination], [false, 'C:j', true, 'B:j', 'B:j']);
  assert.deepEqual(result.routing.afterUse.canAdd, [false, false, true]);
  assert.deepEqual([result.midu.disabled.disabled, result.midu.disabled.placeholder, result.midu.drawn,
    result.midu.enabled.disabled, result.midu.enabled.placeholder, result.midu.rehuomo], [true, 1, 1, false, 0, true]);
}
