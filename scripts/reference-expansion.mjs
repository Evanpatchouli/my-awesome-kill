import assert from 'node:assert/strict';

// Only the upstream page executes the game operations in this fixture.
export function installReferenceExpansion() {
  let owner;
  let material;
  let stage = 'setup';
  let originalTrigger;
  let firstRound;
  let sides;
  const events = [];
  const attacks = [];
  const snapshots = {};
  let observe;

  window.__runReferenceExpansion = async ({ lib, game, ui, get, actor }) => {
    const status = () => ({
      zone: get.position(material, true) || null,
      inHand: owner.getCards('h').includes(material),
      inExpansion: owner.getCards('x').includes(material),
      tagged: owner.getExpansions('zhongyi2').includes(material),
      markCount: owner.getExpansions('zhongyi2').length,
      inDiscard: ui.discardPile.contains(material),
      buff: owner.hasSkill('zhongyi2'),
      spent: owner.awakenedSkills.includes('zhongyi'),
      disabled: owner.disabledSkills.zhongyi?.includes('zhongyi_awake') || false,
    });
    if (!owner) {
      owner = actor;
      firstRound = game.roundNumber;
      const skills = (await import('/character/standard/skill.js')).default;
      for (const skill of ['zhongyi', 'zhongyi2']) {
        if (!lib.skill[skill]) {
          lib.skill[skill] = skills[skill];
          game.finishSkill(skill);
        }
      }
      game.showIdentity();
      // The versus content tests Player.side. Give this diagnostic table two
      // teams before exercising the actual upstream isFriendOf predicate.
      const ordered = [owner];
      for (let player = owner.next; player !== owner; player = player.next) ordered.push(player);
      sides = {
        friendsBeforeConfiguration: ordered.slice(1).filter(player => player.isFriendOf(owner)).length,
        teams: ordered.map((player, i) => (player.side = i % 2 === 0)),
      };
      for (const player of game.players) {
        const held = player.getCards('he');
        if (held.length) await player.discard(held);
        player.hp = player.maxHp = 8;
        player.update();
      }
      owner.addSkill('zhongyi');
      material = game.createCard2('tao', 'heart', 3);
      await owner.gain(material);
      const timings = new Set(['useSkillBegin', 'useSkillAfter', 'addToExpansionBefore', 'addToExpansionAfter',
        'loseAfter', 'loseToDiscardpileBegin', 'loseToDiscardpileAfter', 'roundStart',
        'damageBegin1', 'damageBegin2', 'damageEnd']);
      const seat = player => player === owner ? 'owner' : player ? (player.isFriendOf(owner) ? 'friend' : 'opponent') : null;
      observe = (kind, timing, subject) => {
        if (!timings.has(timing)) return;
        const relatesToMaterial = subject.cards?.includes(material) || subject.skill === 'zhongyi';
        if (!(timing === 'roundStart' || relatesToMaterial || (stage.startsWith('attack-') && subject.name === 'damage'))) return;
        const state = status();
        events.push([game.roundNumber - firstRound + 1, stage, kind, timing, subject.name,
          subject.getParent()?.name || null, seat(subject.player), seat(subject.source), subject.num ?? null,
          state.zone, state.tagged, state.buff, state.spent]);
      };
      const skillId = 'reference_expansion_observer';
      lib.skill[skillId] = {
        trigger: { global: [...timings] }, forced: true, silent: true, firstDo: true,
        async content(e, subject) { observe('effect', e.triggername, subject); },
      };
      game.finishSkill(skillId);
      owner.addSkill(skillId);
      const proto = lib.element.GameEvent.prototype;
      originalTrigger = proto.trigger;
      proto.trigger = function (name) {
        observe('request', name, this);
        return originalTrigger.apply(this, arguments);
      };
      snapshots.before = status();
      stage = 'activate';
      await owner.useSkill({ skill: 'zhongyi', cards: [material], targets: [] });
      snapshots.activated = status();
      stage = 'between';
    }
    const testTarget = actor.next === game.me ? actor.previous : actor.next;
    if (!testTarget || testTarget === game.me) throw new Error('Missing adjacent AI target');
    const hit = async (name, physicalSha) => {
      stage = 'prepare';
      const held = testTarget.getCards('he');
      if (held.length) await testTarget.discard(held);
      stage = 'attack-' + name;
      const before = testTarget.hp;
      let card;
      if (physicalSha) {
        card = game.createCard2('sha', 'spade', 7);
        await actor.gain(card);
        if (!actor.canUse(card, testTarget)) throw new Error('Test Sha has no legal target');
        await actor.useCard(card, testTarget);
      } else {
        await testTarget.damage(1, actor);
      }
      const damage = testTarget.getHistory('damage').at(-1);
      if (!damage || damage.source !== actor) throw new Error('Damage history is missing the actual source');
      attacks.push({ name, physicalSha, friendly: actor.isFriendOf(owner), before, after: testTarget.hp,
        amount: damage.num, parent: damage.getParent()?.name || null,
        sameCard: card ? damage.cards.includes(card) : null, afterState: status() });
      stage = 'between';
    };
    if (game.roundNumber === firstRound) {
      if (actor !== owner) {
        const name = actor.isFriendOf(owner) ? 'friend' : 'opponent';
        if (!attacks.some(hit => hit.name === name)) {
          await hit(name, true);
          if (name === 'friend') await hit('friend-other-damage', false);
        }
      }
      return;
    }
    if (actor !== owner) throw new Error('Unexpected second-round starting seat');
    snapshots.nextRound = status();
    await hit('expired', true);
    snapshots.final = status();
    lib.element.GameEvent.prototype.trigger = originalTrigger;
    owner.removeSkill('reference_expansion_observer');
    return {
      players: game.players.length, rounds: game.roundNumber - firstRound + 1, sides,
      material: [material.name, get.suit(material), get.number(material)], snapshots, attacks,
      eventColumns: ['round', 'operation', 'observation', 'timing', 'event', 'parent', 'player', 'source',
        'amount', 'zone', 'expansionTagged', 'buff', 'limitedSpent'], events,
    };
  };
}

export function checkReferenceExpansion({ rounds, snapshots, attacks, events }) {
  assert.equal(rounds, 2);
  assert.equal(snapshots.before.inHand, true);
  assert.equal(snapshots.before.spent, false);
  assert.equal(snapshots.activated.markCount, 1);
  assert.equal(snapshots.activated.inExpansion, true);
  assert.equal(snapshots.activated.inHand, false);
  assert.equal(snapshots.activated.buff, true);
  assert.equal(snapshots.activated.spent, true);
  assert.equal(snapshots.activated.disabled, true);
  assert.equal(snapshots.nextRound.inDiscard, true);
  assert.equal(snapshots.final.markCount, 0);
  assert.equal(snapshots.final.buff, false);
  assert.equal(snapshots.final.spent, true);
  for (const state of Object.values(snapshots)) {
    assert.equal([state.inHand, state.inExpansion, state.inDiscard].filter(Boolean).length, 1);
  }
  const hit = name => attacks.find(test => test.name === name);
  for (const [name, expected] of [['friend', 2], ['opponent', 1], ['friend-other-damage', 1], ['expired', 1]]) {
    const event = hit(name);
    assert.ok(event, `Missing ${name} in ${attacks.map(hit => hit.name).join(', ')}`);
    assert.equal(event.amount, expected, name);
    assert.equal(event.before - event.after, expected);
    if (event.physicalSha) assert.equal(event.sameCard, true);
  }
  const expiry = events.filter(row => row[0] === 2);
  const cleanup = expiry.filter(row => row[2] === 'request' && row[3] === 'loseToDiscardpileAfter');
  assert.equal(cleanup.length, 1);
  const roundRequest = expiry.find(row => row[2] === 'request' && row[3] === 'roundStart');
  const roundEffect = expiry.find(row => row[2] === 'effect' && row[3] === 'roundStart');
  assert.deepEqual(roundRequest.slice(9), ['x', true, true, true]);
  assert.deepEqual(roundEffect.slice(9), ['d', false, false, true]);
}
