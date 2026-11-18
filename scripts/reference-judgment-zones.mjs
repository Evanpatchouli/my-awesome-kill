import assert from 'node:assert/strict';

// Runs the original delayed-card and judgment implementations; only card
// placement and offline choice preferences are controlled by the fixture.
export function installJudgmentZones() {
  window.__runJudgmentZones = async ({ lib, game, ui, get, _status, actor, target }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const ordered = [target];
    while (ordered.length < game.players.length) ordered.push(ordered.at(-1).next);
    const labels = 'ABCDEFGH';
    const seat = player => player ? labels[ordered.indexOf(player)] || null : null;
    const cards = new Map();
    const events = [], trials = [];
    let sample;
    const zone = card => {
      if (!card) return null;
      const pos = get.position(card, true);
      const owner = get.owner(card);
      return owner && pos ? `${seat(owner)}:${pos}` : pos || null;
    };
    const label = card => cards.get(card) || null;
    const create = (id, kind, suit, rank) => {
      const card = game.createCard2(kind, suit, rank); cards.set(card, id); return card;
    };
    const timings = new Set(['loseBefore', 'loseAfter', 'phaseJudgeBefore', 'phaseJudgeBegin', 'phaseJudge',
      'phaseJudgeEnd', 'phaseJudgeAfter', 'judge', 'judgeFixing', 'judgeEnd', 'cardsDiscardAfter',
      'gainAfter', 'damageBegin1', 'damageEnd', 'chooseToUseBegin', 'chooseToUseAfter']);
    function capture(point, timing, event) {
      if (!sample || (point !== 'content' && !timings.has(timing))) return;
      const judged = event.result?.card || event.player?.judging?.[0];
      events.push({ sample: sample.name, point, timing, event: event.name,
        player: seat(event.player), source: seat(event.source), parent: event.getParent()?.name || null,
        delayed: [label(sample.delayed), zone(sample.delayed)],
        revealed: [label(sample.revealed), zone(sample.revealed)],
        revealedOwner: seat(get.owner(sample.revealed)),
        judgmentCard: label(judged), accepted: event.result?.bool ?? null,
        cards: (event.cards || []).filter(card => cards.has(card)).map(card => [label(card), zone(card)]) });
    }
    const originalTrigger = lib.element.GameEvent.prototype.trigger;
    lib.element.GameEvent.prototype.trigger = function (timing) {
      capture('request', timing, this);
      return originalTrigger.apply(this, arguments);
    };
    const observer = 'reference_judgment_zone_observer';
    lib.skill[observer] = { trigger: { global: [...timings] }, firstDo: true, forced: true, silent: true,
      filter: (_, player) => !!sample && player === game.me,
      async content(event, trigger) { capture('callback', event.triggername, trigger); } };
    game.finishSkill(observer); game.addGlobalSkill(observer);
    for (const name of ['lebu', 'shandian', 'shandianCancel']) {
      game.globalEventHandlers.pushHandler(name, (event, { state }) => {
        if (sample) capture('content', state, event);
      });
    }
    game.globalEventHandlers.pushHandler('chooseToUse', (event, { state }) => {
      if (sample && state === 'begin' && event.type === 'wuxie') {
        event.ai1 = () => sample.counter && event.player === actor ? 100 : -100;
      }
    });
    game.globalEventHandlers.pushHandler('chooseBool', (event, { state }) => {
      if (sample && state === 'begin') event.ai = () => sample.tiandu ? 100 : -100;
    });
    try {
      for (const player of game.players) {
        const held = player.getCards('hej');
        if (held.length) await player.discard(held);
        player.hp = player.maxHp = 8; player.update();
      }
      for (const [name, kind, suit, rank, counter, tiandu] of [
        ['lebu-fail', 'lebu', 'club', 8, false, false],
        ['lebu-pass-tiandu', 'lebu', 'heart', 8, false, true],
        ['lightning-hit', 'shandian', 'spade', 8, false, false],
        ['lightning-pass', 'shandian', 'heart', 8, false, false],
        ['lightning-counter', 'shandian', 'spade', 8, true, false],
        ['lebu-counter', 'lebu', 'club', 8, true, false],
      ]) {
        for (const player of game.players) {
          const cards = player.getCards('hej'); if (cards.length) await player.discard(cards);
        }
        const delayed = create(`${name}-delayed`, kind, 'club', 6);
        const revealed = create(`${name}-revealed`, 'sha', suit, rank);
        const wuxie = counter ? create(`${name}-wuxie`, 'wuxie', 'spade', 11) : null;
        if (wuxie) await actor.gain(wuxie);
        if (tiandu) target.addSkill('tiandu');
        await target.addJudge(delayed);
        ui.cardPile.prepend(revealed);
        sample = { name, delayed, revealed, counter, tiandu };
        const before = target.hp;
        await target.phaseJudge();
        trials.push({ name, before, after: target.hp, delayed: zone(delayed), revealed: zone(revealed),
          wuxie: zone(wuxie), skipped: target.skipList.slice(), judgeCount: target.getCards('j').length });
        sample = null;
        if (tiandu) target.removeSkill('tiandu');
        target.skipList.length = 0;
      }
      return { players: game.players.length, trials, events };
    } finally {
      lib.element.GameEvent.prototype.trigger = originalTrigger;
      game.removeGlobalSkill(observer);
    }
  };
}

export function checkJudgmentZones(result) {
  assert.equal(result.players, 8);
  assert.deepEqual(result.trials.map(row => row.name), ['lebu-fail', 'lebu-pass-tiandu', 'lightning-hit', 'lightning-pass', 'lightning-counter', 'lebu-counter']);
  const [fail, tiandu, hit, pass, cancelledLightning, cancelledLebu] = result.trials;
  assert.deepEqual([fail.delayed, fail.revealed, fail.skipped], ['d', 'd', ['phaseUse']]);
  assert.deepEqual([tiandu.delayed, tiandu.revealed, tiandu.skipped], ['d', 'A:h', []]);
  assert.deepEqual([hit.before - hit.after, hit.delayed, hit.revealed], [3, 'd', 'd']);
  assert.deepEqual([pass.delayed, pass.revealed], ['B:j', 'd']);
  assert.deepEqual([cancelledLightning.delayed, cancelledLightning.revealed, cancelledLightning.wuxie], ['B:j', 'c', 'd']);
  assert.deepEqual([cancelledLebu.delayed, cancelledLebu.revealed, cancelledLebu.wuxie, cancelledLebu.skipped], ['d', 'c', 'd', []]);
  for (const trial of result.trials) {
    const rows = result.events.filter(row => row.sample === trial.name && row.point === 'request');
    const at = (timing, predicate = () => true) => rows.findIndex(row => row.timing === timing && predicate(row));
    const loss = at('loseAfter', row => row.parent === 'phaseJudge');
    const stage = at('phaseJudge');
    assert.ok(loss >= 0 && stage > loss);
    assert.deepEqual(rows[loss].delayed, [`${trial.name}-delayed`, 'o']);
    assert.deepEqual(rows[stage].delayed, [`${trial.name}-delayed`, 'o']);
    const judge = at('judge');
    if (trial.name.endsWith('counter')) {
      assert.equal(judge, -1);
      continue;
    }
    const fixing = at('judgeFixing'), end = at('judgeEnd');
    assert.ok(judge > stage && fixing > judge && end > fixing);
    assert.equal(rows[judge].parent, 'phaseJudge');
    assert.equal(rows[judge].judgmentCard, `${trial.name}-revealed`);
    assert.equal(rows[judge].revealedOwner, 'A');
    assert.equal(rows[fixing].revealed[1], 'o');
    assert.equal(rows[end].revealed[1], 'o');
    if (trial.name === 'lebu-pass-tiandu') {
      assert.ok(at('gainAfter', row => row.parent === 'tiandu' && row.revealed[1] === 'A:h') > end);
    } else {
      assert.ok(at('cardsDiscardAfter', row => row.cards.some(([id]) => id === `${trial.name}-revealed`)) > end);
    }
    if (trial.name === 'lightning-hit') {
      const damage = rows[at('damageBegin1')];
      assert.equal(damage.source, null);
      assert.deepEqual(damage.cards, [[`${trial.name}-delayed`, 'o']]);
    }
    if (trial.delayed === 'd') {
      const phaseAfter = at('phaseJudgeAfter');
      const delayedDiscard = at('cardsDiscardAfter', row => row.cards.some(([id]) => id === `${trial.name}-delayed`));
      assert.ok(delayedDiscard > phaseAfter);
    }
  }
}
