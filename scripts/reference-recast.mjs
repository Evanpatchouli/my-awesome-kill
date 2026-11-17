// Passed directly to page.evaluate before the shared phase-use fixture runs.
export function installReferenceRecast() {
  window.__runReferenceRecast = async ({ lib, game, ui, get, actor, target }) => {
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
    proto.trigger = function (hook) {
      if (phase) {
        events.push([phase, hook, this.name, this.getParent()?.name || null, label(this.player),
          label(this.source), label(this.target), zone(observedMaterial), zone(observedReplacement), actor.countCards('h')]);
      }
      return original.apply(this, arguments);
    };
    const callbacks = [];
    try {
      const defaultEvent = actor.recast(material);
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
      const overrideEvent = actor.recast(overrideMaterial,
        (player, cards) => { callbacks.push({ kind: 'lose', count: cards.length }); return player.give(cards, target); },
        (player, cards) => { callbacks.push({ kind: 'gain', count: cards.length }); return player.gain(overrideReplacement, 'gain2'); });
      await overrideEvent;
      return {
        setup, afterDefault, overrideSetup, callbacks,
        afterOverride: {
          material: location(overrideMaterial), replacement: location(overrideReplacement),
          handSize: actor.countCards('h'), useCardStat: stat(), targetLinked: target.isLinked(),
          actorLinked: actor.isLinked(), materialLocations: count(overrideMaterial), replacementLocations: count(overrideReplacement),
          losingEvents: overrideEvent.recastingLosingEvents.map(event => event.name),
          gainingEvents: overrideEvent.recastingGainingEvents.map(event => event.name),
        },
        eventColumns: ['phase', 'hook', 'event', 'parent', 'player', 'source', 'target', 'material', 'replacement', 'actorHandSize'],
        events,
      };
    } finally {
      proto.trigger = original;
    }
  };
}
