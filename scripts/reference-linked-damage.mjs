import assert from 'node:assert/strict';

// Runs in the upstream browser. Each sample starts in a separate phaseUseBegin.
export function installReferenceLinkedDamage() {
  const samples = ['unarmored', 'armored', 'recheck'];
  const trials = [];
  const events = [];

  window.__runReferenceLinkedDamage = async ({ lib, game, ui, get, _status, actor, target }) => {
    const sample = samples[trials.length];
    const others = [];
    for (let seat = actor.next; seat !== actor; seat = seat.next) {
      if (seat !== target && seat !== game.me) others.push(seat);
    }
    const [nextTarget, lastTarget] = others;
    const seats = { A: actor, B: target, C: nextTarget };
    if (sample === 'recheck') seats.D = lastTarget;
    const label = player => Object.keys(seats).find(key => seats[key] === player) || (player ? 'other' : null);
    const face = card => [card.name, get.suit(card), get.number(card)];
    const states = () => Object.fromEntries(Object.entries(seats).map(([key, player]) => [
      key, { hp: player.hp, hujia: player.hujia, linked: player.isLinked(), handSize: player.countCards('h') },
    ]));

    // Only these test seats receive expansion content. The ordinary engine
    // owns the actual card effect, rule skills, optional choice and damage.
    if (!lib.card.tiesuo) lib.card.tiesuo = (await import('/card/extra.js')).default.card.tiesuo;
    if (!lib.skill.faen) {
      lib.skill.faen = (await import('/character/yijiang/skill.js')).default.faen;
      game.finishSkill('faen');
    }
    if (game.players.some(player => player.isLinked())) throw new Error('Unexpected link before setup');
    for (const player of game.players) {
      const cards = player.getCards('he');
      if (cards.length) await player.discard(cards);
      player.maxHp = 4;
      player.hp = 4;
      player.hujia = 0;
      player.update();
    }
    if (sample === 'armored') await target.changeHujia(1);
    if (sample === 'recheck') await lastTarget.link(true);
    game.showIdentity();
    target.addSkill('faen');
    const tiesuo = game.createCard2('tiesuo', 'club', 10);
    const fireSha = game.createCard2('sha', 'heart', 4, 'fire');
    await actor.gain([tiesuo, fireSha], 'gain2');
    // Neither known draw can dodge the sha or counter the second tiesuo effect.
    const draws = [game.createCard2('tao', 'heart', 7), game.createCard2('tao', 'heart', 8)];
    for (const card of draws.toReversed()) ui.cardPile.insertBefore(card, ui.cardPile.firstChild);
    const initial = {
      states: states(), skills: Object.fromEntries(Object.entries(seats).map(([key, player]) => [key, player.getSkills('invisible')])),
      turnOwner: label(_status.currentPhase), targetIsHuman: target.isMine(),
      actorHand: actor.getCards('h').map(face), fireNature: get.nature(fireSha),
      canUseTiesuo: [target, nextTarget].map(player => actor.canUse(tiesuo, player)),
      canUseFireSha: actor.canUse(fireSha, target), priorShaUses: actor.getStat('card').sha || 0,
    };
    if (initial.turnOwner !== 'A' || initial.targetIsHuman || !initial.canUseFireSha || initial.priorShaUses ||
        !initial.canUseTiesuo.every(Boolean) || initial.fireNature !== 'fire' || !target.hasSkill('faen')) {
      throw new Error(`Invalid linked setup: ${JSON.stringify(initial)}`);
    }

    const callbacks = ['linkAfter', 'changeHp', 'damage', 'damageSource', 'damageEnd', 'damageAfter'];
    const requests = new Set([...callbacks, 'linkBefore', 'changeHujiaAfter', 'useCardAfter']);
    const record = (kind, timing, event) => events.push([
      sample, kind, timing, event.name, event.getParent()?.name || null, label(event.player),
      label(event.source), event.card?.name || null, event.nature || null, event.num ?? null,
      event.player.hp, event.player.hujia, event.player.isLinked(),
    ]);
    const observer = 'reference_link_observer';
    lib.skill[observer] = {
      trigger: { global: callbacks }, silent: true, forced: true, firstDo: true,
      async content(event, trigger) { record('callback', event.triggername, trigger); },
    };
    game.finishSkill(observer);
    actor.addSkill(observer);
    let recheck;
    const release = 'reference_unlink_pending';
    if (sample === 'recheck') {
      lib.skill[release] = {
        trigger: { player: 'damageEnd' }, silent: true, forced: true,
        async content(event, trigger) {
          const spread = trigger.getParent('_lianhuan');
          recheck = { duringDamageTo: label(trigger.player), pending: spread.targets.map(label), linkedBefore: lastTarget.isLinked() };
          await lastTarget.link(false);
          recheck.linkedAfter = lastTarget.isLinked();
        },
      };
      game.finishSkill(release);
      nextTarget.addSkill(release);
    }

    const choices = [];
    const effects = [];
    const before = new WeakMap();
    // Upstream event-content handlers bracket each actual content step.
    function observeContent(event, { state }) {
      if (event.name === 'chooseBool') {
        const parent = event.getParent();
        if (state === 'end' && parent.skill === 'faen') {
          choices.push({ owner: label(event.player), target: label(parent._trigger.player), accepted: event.result.bool });
        }
        return;
      }
      const recipient = event.name === 'faen' ? event._trigger.player : event.target;
      if (state === 'begin') {
        before.set(event, { linked: recipient.isLinked(), hand: recipient.getCards('h') });
      } else {
        const prior = before.get(event);
        const effect = { name: event.name, owner: label(event.player), target: label(recipient),
          linkedBefore: prior.linked, linkedAfter: recipient.isLinked() };
        if (event.name === 'faen') {
          effect.timing = event.triggername;
          effect.drawn = recipient.getCards('h').filter(card => !prior.hand.includes(card)).map(face);
        }
        effects.push(effect);
      }
    }
    const observedContent = ['chooseBool', 'faen', 'tiesuo'];
    for (const name of observedContent) game.globalEventHandlers.pushHandler(name, observeContent);
    const proto = lib.element.GameEvent.prototype;
    const original = proto.trigger;
    proto.trigger = function (timing) {
      if (requests.has(timing) && this.player) record('request', timing, this);
      return original.apply(this, arguments);
    };
    try {
      await actor.useCard(tiesuo, [target, nextTarget]);
      const afterTiesuo = states();
      await actor.useCard(fireSha, target);
      const damage = actor.getHistory('sourceDamage').map(event => ({
        source: label(event.source), target: label(event.player), nature: event.nature, amount: event.num,
        absorbed: event.hujia || 0, parent: event.getParent().name,
        physicalSha: event.cards?.includes(fireSha) === true, inTargetHistory: event.player.getHistory('damage').includes(event),
      }));
      trials.push({
        sample, turn: game.phaseNumber, initial, afterTiesuo, choices, effects, damage, ...(recheck ? { recheck } : {}),
        final: {
          states: states(),
          damageCounts: Object.fromEntries(Object.entries(seats).map(([key, player]) => [key, player.getHistory('damage').length])),
          tiesuoInDiscard: ui.discardPile.contains(tiesuo), shaInDiscard: ui.discardPile.contains(fireSha),
          tiesuoUses: actor.getStat('card').tiesuo, shaUses: actor.getStat('card').sha,
        },
      });
    } finally {
      proto.trigger = original;
      for (const name of observedContent) {
        game.globalEventHandlers.removeHandler(name, game.globalEventHandlers.getDefaultHandlerType(name), observeContent);
      }
      actor.removeSkill(observer);
      target.removeSkill('faen');
      if (sample === 'recheck') nextTarget.removeSkill(release);
    }
    if (trials.length === samples.length) {
      return {
        players: game.players.length, trials,
        eventColumns: ['sample', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'card', 'nature', 'amount', 'hp', 'hujia', 'linked'],
        events,
      };
    }
  };
}

export function checkReferenceLinkedDamage({ players, trials, events, eventColumns }) {
  assert.equal(players, 8);
  assert.deepEqual(trials.map(({ sample, turn }) => [sample, turn]), [['unarmored', 1], ['armored', 2], ['recheck', 3]]);
  const rows = events.map(row => Object.fromEntries(eventColumns.map((key, index) => [key, row[index]])));
  for (const trial of trials) {
    const { sample, initial, afterTiesuo, choices, effects, damage, final } = trial;
    const armor = sample === 'armored' ? 1 : 0;
    assert.deepEqual(initial.actorHand, [['tiesuo', 'club', 10], ['sha', 'heart', 4]]);
    assert.deepEqual(initial.skills.B, ['faen']);
    assert.deepEqual(initial.states.B, { hp: 4, hujia: armor, linked: false, handSize: 0 });
    assert.deepEqual(initial.states.C, { hp: 4, hujia: 0, linked: false, handSize: 0 });
    assert.deepEqual(effects.filter(effect => effect.name === 'tiesuo').map(effect => [effect.target, effect.linkedBefore, effect.linkedAfter]), [
      ['B', false, true], ['C', false, true],
    ]);
    assert.deepEqual(choices.map(choice => choice.target), ['B', 'C']);
    const faen = effects.filter(effect => effect.name === 'faen');
    assert.deepEqual(faen.map(effect => effect.target), choices.filter(choice => choice.accepted).map(choice => choice.target));
    for (const effect of faen) {
      assert.equal(effect.owner, 'B');
      assert.equal(effect.timing, 'linkAfter');
      assert.equal(effect.linkedBefore, true);
      assert.equal(effect.drawn.length, 1);
      assert.equal(effect.drawn[0][0], 'tao');
    }
    assert.deepEqual(damage, ['B', 'C'].map((target, i) => ({
      source: 'A', target, nature: 'fire', amount: 1, absorbed: i ? 0 : armor, parent: i ? '_lianhuan' : 'sha',
      physicalSha: true, inTargetHistory: true,
    })));
    assert.equal(final.states.B.hp, 3 + armor);
    assert.equal(final.states.B.hujia, 0);
    assert.equal(final.states.C.hp, 3);
    for (const target of ['B', 'C']) {
      assert.equal(afterTiesuo[target].linked, true);
      assert.equal(final.states[target].linked, false);
      assert.equal(final.states[target].handSize, faen.filter(effect => effect.target === target).length);
      assert.equal(final.damageCounts[target], 1);
    }
    assert.equal(final.tiesuoInDiscard && final.shaInDiscard, true);
    assert.deepEqual([final.tiesuoUses, final.shaUses], [1, 1]);
    const direct = rows.filter(row => row.sample === sample && row.player === 'B' && row.timing === 'damage');
    assert.deepEqual(direct.map(row => [row.kind, row.hp, row.hujia, row.linked]), [
      ['request', 4, armor, true], ['callback', 3 + armor, 0, false],
    ]);
  }
  const [normal, armored, recheck] = trials;
  assert.deepEqual(normal.choices.map(choice => choice.accepted), [true, true]);
  assert.deepEqual(armored.choices.map(choice => choice.accepted), [true, false]);
  assert.deepEqual(recheck.recheck, { duringDamageTo: 'C', pending: ['D'], linkedBefore: true, linkedAfter: false });
  assert.deepEqual(recheck.final.states.D, { hp: 4, hujia: 0, linked: false, handSize: 0 });
  assert.equal(recheck.final.damageCounts.D, 0);
}
