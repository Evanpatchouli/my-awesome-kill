import assert from 'node:assert/strict';

// Passed directly to page.evaluate before the shared phase-use fixture runs.
export function installReferenceRecast() {
  window.__runReferenceRecast = async ({ lib, game, ui, get, actor, target, observeCallbacks = false }) => {
    const existing = actor.getCards('he');
    if (existing.length) await actor.discard(existing);
    const material = game.createCard2('tiesuo', 'club', 10);
    const control = game.createCard2('sha', 'spade', 7);
    const replacement = game.createCard2('tao', 'heart', 2);
    await actor.gain([material, control], 'gain2');
    ui.cardPile.insertBefore(replacement, ui.cardPile.firstChild);

    const location = card => ({
      position: get.position(card, true),
      actorHand: actor.getCards('h').includes(card),
      targetHand: target.getCards('h').includes(card),
      discard: ui.discardPile.contains(card),
      pile: ui.cardPile.contains(card),
    });
    const count = card => [actor.getCards('h').includes(card), target.getCards('h').includes(card),
      ui.discardPile.contains(card), ui.cardPile.contains(card)].filter(Boolean).length;
    const zone = card => {
      if (actor.getCards('h').includes(card)) return 'A:h';
      if (target.getCards('h').includes(card)) return 'B:h';
      return get.position(card, true) || null;
    };
    const stat = () => actor.getStat('card').tiesuo || 0;
    const label = player => player === actor ? 'actor' : player === target ? 'target' : player ? 'other' : null;
    const setup = {
      material: { name: material.name, ...location(material), strict: actor.canRecast(material, null, true) },
      control: { name: control.name, ...location(control), strict: actor.canRecast(control, null, true) },
      replacement: location(replacement), handSize: actor.countCards('h'), useCardStat: stat(),
      targetLinked: target.isLinked(), actorLinked: actor.isLinked(),
    };
    const events = [];
    const proto = lib.element.GameEvent.prototype;
    const original = proto.trigger;
    let phase = 'default';
    let observedMaterial = material;
    let observedReplacement = replacement;
    const observer = game.players.find(player => player !== actor && player !== target);
    const timings = new Set(['recastingLose', 'recastingLost', 'recast', 'recastingGain', 'recastingGained', 'loseToDiscardpile']);
    for (const name of ['recast', 'loseToDiscardpile', 'lose', 'draw', 'gain']) {
      for (const suffix of ['Before', 'Begin', 'End', 'After', 'Cancelled']) timings.add(name + suffix);
    }
    const observationSkill = 'reference_recast_observe';
    const observe = (kind, timing, subject) => {
      if (!phase) return;
      const cards = (subject.cards || []).map(card => {
        if (card === observedMaterial) return 'material';
        if (card === observedReplacement) return 'replacement';
        return card.name;
      });
      events.push([phase, kind, timing, subject.name, subject.getParent()?.name || null,
        label(subject.player), label(subject.source), label(subject.target), zone(observedMaterial),
        zone(observedReplacement), actor.countCards('h'), cards, actor.getKnownCards(observer).includes(observedReplacement)]);
    };
    let intercepted = false;
    if (observeCallbacks) {
      lib.skill[observationSkill] = {
        trigger: { global: [...timings] }, firstDo: true, forced: true, silent: true,
        async content(event, subject) {
          const timing = event.triggername;
          observe('effect', timing, subject);
          if (phase === 'cancel-after-loss' && timing === 'recast') subject.cancel();
          if (phase === 'material-intercept' && timing === 'loseBefore' && !intercepted &&
              subject.player === actor && subject.cards?.includes(observedMaterial)) {
            intercepted = true;
            await actor.give(observedMaterial, target);
          }
        },
      };
      game.finishSkill(observationSkill);
      actor.addSkill(observationSkill);
    }
    const instrument = recastEvent => {
      if (observeCallbacks) {
        for (const name of ['recastingLose', 'recastingGain']) {
          const factory = recastEvent[name];
          recastEvent[name] = function (...args) {
            observe('factory', name, recastEvent);
            return factory.apply(this, args);
          };
        }
      }
      return recastEvent;
    };
    proto.trigger = function (hook) {
      if (observeCallbacks) {
        if (timings.has(hook)) observe('request', hook, this);
      } else if (phase) {
        events.push([phase, hook, this.name, this.getParent()?.name || null, label(this.player),
          label(this.source), label(this.target), zone(observedMaterial), zone(observedReplacement), actor.countCards('h')]);
      }
      return original.apply(this, arguments);
    };
    const callbacks = [];
    try {
      const defaultEvent = instrument(actor.recast(material));
      await defaultEvent;
      const afterDefault = {
        material: location(material), replacement: location(replacement), control: location(control),
        handSize: actor.countCards('h'), useCardStat: stat(), targetLinked: target.isLinked(),
        actorLinked: actor.isLinked(), materialLocations: count(material), replacementLocations: count(replacement),
        losingEvents: defaultEvent.recastingLosingEvents.map(event => event.name),
        gainingEvents: defaultEvent.recastingGainingEvents.map(event => event.name),
      };

      phase = null;
      const overrideMaterial = game.createCard2('tiesuo', 'club', 11);
      const overrideReplacement = game.createCard2('shan', 'heart', 3);
      await actor.gain(overrideMaterial, 'gain2');
      ui.cardPile.insertBefore(overrideReplacement, ui.cardPile.firstChild);
      const overrideSetup = { material: location(overrideMaterial), replacement: location(overrideReplacement), handSize: actor.countCards('h'), useCardStat: stat() };
      phase = 'override';
      observedMaterial = overrideMaterial;
      observedReplacement = overrideReplacement;
      const overrideEvent = instrument(actor.recast(overrideMaterial,
        (player, cards) => { callbacks.push({ kind: 'lose', count: cards.length }); return player.give(cards, target); },
        (player, cards) => { callbacks.push({ kind: 'gain', count: cards.length }); return player.gain(overrideReplacement, 'gain2'); }));
      await overrideEvent;
      const afterOverride = {
        material: location(overrideMaterial), replacement: location(overrideReplacement),
        handSize: actor.countCards('h'), useCardStat: stat(), targetLinked: target.isLinked(),
        actorLinked: actor.isLinked(), materialLocations: count(overrideMaterial), replacementLocations: count(overrideReplacement),
        losingEvents: overrideEvent.recastingLosingEvents.map(event => event.name),
        gainingEvents: overrideEvent.recastingGainingEvents.map(event => event.name),
      };
      const interrupted = [];
      if (observeCallbacks) {
        for (const name of ['cancel-after-loss', 'material-intercept']) {
          phase = null;
          observedMaterial = game.createCard2('tiesuo', 'club', 12);
          observedReplacement = game.createCard2('tao', 'heart', 4);
          await actor.gain(observedMaterial);
          ui.cardPile.insertBefore(observedReplacement, ui.cardPile.firstChild);
          const startingHand = actor.countCards('h');
          phase = name;
          await instrument(actor.recast(observedMaterial));
          interrupted.push({ phase, startingHand, endingHand: actor.countCards('h'),
            material: zone(observedMaterial), replacement: zone(observedReplacement), useCardStat: stat() });
        }
      }
      return {
        setup, afterDefault, overrideSetup, callbacks, afterOverride,
        ...(observeCallbacks ? { interrupted } : {}),
        eventColumns: observeCallbacks
          ? ['phase', 'observation', 'timing', 'event', 'parent', 'player', 'source', 'target', 'material', 'replacement', 'actorHandSize', 'subjectCards', 'observerKnowsReplacementInHand']
          : ['phase', 'hook', 'event', 'parent', 'player', 'source', 'target', 'material', 'replacement', 'actorHandSize'],
        events,
      };
    } finally {
      proto.trigger = original;
      if (observeCallbacks) actor.removeSkill(observationSkill);
    }
  };
}

export function checkReferenceRecastCallbacks(result) {
  const columns = Object.fromEntries(result.eventColumns.map((name, index) => [name, index]));
  for (const phase of ['default', 'override']) {
    const rows = result.events.filter(row => row[columns.phase] === phase);
    for (const [pre, post] of [['recastingLose', 'recastingLost'], ['recastingGain', 'recastingGained']]) {
      const at = (kind, timing) => rows.findIndex(row =>
        row[columns.observation] === kind && row[columns.timing] === timing);
      const positions = [at('request', pre), at('factory', pre), at('request', post), at('effect', pre), at('effect', post)];
      assert.ok(positions.every((position, i) => position >= 0 && (i === 0 || position > positions[i - 1])),
        `${phase} must call the factory before dispatching either queued skill`);
      const request = rows[at('request', post)];
      const executed = rows[at('effect', post)];
      if (post === 'recastingLost') {
        assert.equal(request[columns.material], 'A:h');
        assert.equal(executed[columns.material], phase === 'default' ? 'd' : 'B:h');
      } else {
        assert.equal(request[columns.replacement], 'c');
        assert.equal(executed[columns.replacement], 'A:h');
        assert.equal(executed[columns.observerKnowsReplacementInHand], phase === 'override');
      }
    }
  }
  const [cancelled, intercepted] = result.interrupted;
  assert.equal(cancelled.phase, 'cancel-after-loss');
  assert.equal(cancelled.endingHand, cancelled.startingHand - 1);
  assert.equal(cancelled.material, 'd');
  assert.equal(cancelled.replacement, 'c');
  assert.equal(intercepted.phase, 'material-intercept');
  assert.equal(intercepted.endingHand, intercepted.startingHand);
  assert.equal(intercepted.material, 'B:h');
  assert.equal(intercepted.replacement, 'A:h');
}
