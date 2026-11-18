import assert from 'node:assert/strict';

export function installZhongyiTransitDeath() {
  window.__runZhongyiTransitDeath = async ({ lib, game, ui, get, _status, actor }) => {
    const holder = game.players.find(player => player !== actor && player !== game.me && player !== game.zhu);
    if (!holder || _status.currentPhase !== actor) throw new Error('Expected a non-lord holder outside their turn');
    const definitions = (await import('/character/standard/skill.js')).default;
    for (const name of ['zhongyi', 'zhongyi2']) {
      if (!lib.skill[name]) {
        lib.skill[name] = definitions[name];
        game.finishSkill(name);
      }
    }
    const card = game.createCard2('qinggang', 'spade', 6);
    await holder.equip(card);
    holder.addSkill('zhongyi');

    const seat = player => player === holder ? 'holder' : player === actor ? 'current-turn' : player ? 'other' : null;
    const snapshot = () => ({
      zone: get.position(card, true), owner: seat(get.owner(card)), parentNode: card.parentNode?.id || null,
      special: ui.special.contains(card), discard: ui.discardPile.contains(card), ordering: ui.ordering.contains(card),
      equipped: holder.getCards('e').includes(card), inExpansion: holder.getCards('x').includes(card),
      knownToSpectator: card.isKnownBy(game.me), alive: holder.isAlive(),
      tag: card.hasGaintag('zhongyi2'), mark: !!holder.marks.zhongyi2, buff: holder.hasSkill('zhongyi2'),
      spent: holder.awakenedSkills.includes('zhongyi'), disabled: !!holder.disabledSkills.zhongyi?.includes('zhongyi_awake'),
    });
    const status = event => ({
      finished: event.finished, cancelled: !!event._cancelled, forceDie: !!event.forceDie,
      result: event.result?.bool ?? null, hasMaterial: !!event.cards?.includes(card),
    });
    const events = [];
    let expansion;
    const capture = (kind, timing, subject) => events.push({
      kind, timing, event: subject.name, parent: subject.getParent()?.name || null,
      ...snapshot(), ...(subject === expansion ? { expansion: status(subject) } : {}),
    });

    const watched = new Set(['useSkillBegin', 'useSkillEnd', 'useSkillAfter', 'addToExpansionBefore',
      'addToExpansionBegin', 'addToExpansionEnd', 'addToExpansionAfter', 'addToExpansionCancelled',
      'loseBefore', 'loseAfter', 'loseToDiscardpileBefore', 'loseToDiscardpileAfter', 'dieBefore', 'die', 'dieAfter']);
    const proto = lib.element.GameEvent.prototype;
    const originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (this.player === holder && watched.has(timing) && (this.name === 'die' || this.cards?.includes(card))) {
        if (timing === 'addToExpansionBefore') expansion = this;
        capture('request', timing, this);
      }
      return originalTrigger.apply(this, arguments);
    };

    const observer = 'reference_zhongyi_transit_death';
    lib.skill[observer] = {
      trigger: { global: ['loseAfter', 'die', 'dieAfter', 'addToExpansionAfter'] },
      forced: true, silent: true, firstDo: true, forceDie: true,
      filter(event, player, timing) {
        if (event.player !== holder) return false;
        if (timing === 'loseAfter') return event.getParent() === expansion && event.cards.includes(card);
        if (timing === 'addToExpansionAfter') return event === expansion;
        return event.name === 'die';
      },
      async content(event, trigger) {
        capture('callback', event.triggername, trigger);
        if (event.triggername === 'loseAfter') {
          if (get.position(card, true) !== 's' || !ui.special.contains(card)) throw new Error('Material is outside the special pile');
          await holder.die().set('animate', false);
          capture('callback', 'returned-from-die', trigger);
        }
      },
    };
    game.finishSkill(observer);
    actor.addSkill(observer);
    const before = snapshot();
    const use = holder.useSkill({ skill: 'zhongyi', cards: [card], targets: [] });
    try {
      await use;
      if (!expansion) throw new Error('Expected the outer expansion event');
      return { material: [card.name, get.suit(card), get.number(card)], slots: game.players.length + game.dead.length,
        remainingPlayers: game.players.length, turnOwner: seat(_status.currentPhase), gameOver: !!_status.over,
        holderIsLord: holder === game.zhu, before, after: snapshot(), outer: status(expansion), use: status(use), events };
    } finally {
      proto.trigger = originalTrigger;
      actor.removeSkill(observer);
    }
  };
}

export function checkZhongyiTransitDeath(result) {
  assert.deepEqual(result.material, ['qinggang', 'spade', 6]);
  assert.equal(result.slots, 8);
  assert.equal(result.remainingPlayers, 7);
  assert.equal(result.turnOwner, 'current-turn');
  assert.equal(result.gameOver, false);
  assert.equal(result.holderIsLord, false);
  assert.deepEqual(result.before, {
    zone: 'e', owner: 'holder', parentNode: null, special: false, discard: false, ordering: false,
    equipped: true, inExpansion: false, knownToSpectator: true, alive: true,
    tag: false, mark: false, buff: false, spent: false, disabled: false,
  });
  const materialInTransit = {
    zone: 's', owner: null, parentNode: 'special', special: true, discard: false, ordering: false,
    equipped: false, inExpansion: false, knownToSpectator: true, alive: true,
    tag: false, mark: false, buff: true, spent: true, disabled: true,
  };
  const afterDeath = { ...materialInTransit, alive: false, buff: false };
  assert.deepEqual(result.after, afterDeath);
  const outer = { finished: true, cancelled: false, forceDie: false, result: null, hasMaterial: true };
  assert.deepEqual(result.outer, outer);
  assert.deepEqual(result.use, { ...outer, forceDie: true });

  assert.deepEqual(result.events.map(({ kind, timing }) => [kind, timing]), [
    ['request', 'useSkillBegin'], ['request', 'addToExpansionBefore'], ['request', 'addToExpansionBegin'],
    ['request', 'loseBefore'], ['request', 'loseAfter'], ['callback', 'loseAfter'],
    ['request', 'dieBefore'], ['request', 'die'], ['callback', 'die'], ['request', 'dieAfter'],
    ['callback', 'dieAfter'], ['callback', 'returned-from-die'], ['request', 'addToExpansionEnd'],
    ['request', 'addToExpansionAfter'], ['callback', 'addToExpansionAfter'],
    ['request', 'useSkillEnd'], ['request', 'useSkillAfter'],
  ]);
  const callback = timing => result.events.find(row => row.kind === 'callback' && row.timing === timing);
  const state = row => Object.fromEntries(Object.keys(materialInTransit).map(key => [key, row[key]]));
  assert.equal(callback('loseAfter').parent, 'addToExpansion');
  assert.deepEqual(state(callback('loseAfter')), materialInTransit);
  assert.deepEqual(state(callback('die')), { ...materialInTransit, alive: false });
  for (const timing of ['dieAfter', 'returned-from-die', 'addToExpansionAfter']) {
    assert.deepEqual(state(callback(timing)), afterDeath);
  }
  assert.deepEqual(callback('addToExpansionAfter').expansion, outer);
}
