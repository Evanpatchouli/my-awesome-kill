import assert from 'node:assert/strict';

export function installReferenceHp() {
  window.__runReferenceHp = async ({ lib, game, ui, actor, target, human }) => {
    const other = game.players.find(player => player !== actor && player !== target && player !== human);
    const seats = { A: actor, B: target, C: other };
    const label = player => Object.keys(seats).find(key => seats[key] === player) || null;
    const state = player => [player.hp, player.hujia, player.isLinked()];
    const events = [], trials = [];
    const times = [
      'damageBefore', 'damageBegin4', 'damage', 'damageSource', 'damageEnd', 'damageAfter', 'damageCancelled',
      'changeHpBefore', 'changeHpBegin', 'changeHp', 'changeHpEnd', 'changeHpAfter', 'changeHpCancelled',
      'changeHujiaBefore', 'changeHujiaAfter', 'linkAfter', 'shaDamage', 'useCardAfter',
    ];
    for (const player of game.players) {
      const held = player.getCards('he');
      if (held.length) await player.discard(held);
    }
    for (const cancelAt of ['changeHpBefore', 'changeHpEnd']) {
      for (const player of game.players) {
        if (player.isLinked()) await player.link(false);
        player.hp = player.maxHp = 4;
        player.hujia = 0;
        player.update();
      }
      await target.changeHujia(1);
      await target.link(true);
      await other.link(true);
      const sha = game.createCard2('sha', 'heart', 4, 'fire');
      await actor.gain(sha);
      const initial = [state(target), state(other)];
      const hpSubjects = new Map();
      const damageFor = event => event.name === 'damage' ? event : event.getParent('damage', true);
      function record(kind, timing, event) {
        const hit = damageFor(event);
        if (event.name === 'changeHp') {
          const prior = hpSubjects.get(event.player);
          if (prior && prior !== event) throw new Error('HP timings did not refer to the same event');
          hpSubjects.set(event.player, event);
        }
        events.push([cancelAt, kind, timing, event.name, event.getParent()?.name || null, label(event.player),
          label(event.source), event.num ?? null, ...state(event.player),
          hit ? hit.player.getHistory('damage').includes(hit) : null,
          hit ? actor.getHistory('sourceDamage').includes(hit) : null,
          hit?.hujia ?? null]);
      }
      const observe = 'reference_hp_observer', stop = 'reference_hp_cancel';
      lib.skill[observe] = {
        trigger: { global: times }, firstDo: true, forced: true, silent: true,
        async content(event, trigger) { record('callback', event.triggername, trigger); },
      };
      lib.skill[stop] = {
        trigger: { player: cancelAt }, forced: true, silent: true,
        filter: event => event.getParent()?.name === 'damage',
        async content(event, trigger) { trigger.cancel(); },
      };
      game.finishSkill(observe);
      game.finishSkill(stop);
      actor.addSkill(observe);
      target.addSkill(stop);
      const proto = lib.element.GameEvent.prototype;
      const original = proto.trigger;
      proto.trigger = function(timing) {
        if (times.includes(timing) && this.player) record('request', timing, this);
        return original.apply(this, arguments);
      };
      try {
        const use = actor.useCard(sha, target, false);
        use.baseDamage = 2;
        await use;
      } finally {
        proto.trigger = original;
        actor.removeSkill(observe);
        target.removeSkill(stop);
      }
      const hits = actor.getHistory('sourceDamage').filter(hit => hit.cards?.includes(sha)).map(hit => ({
        target: label(hit.player), source: label(hit.source), amount: hit.num, nature: hit.nature,
        absorbed: hit.hujia || 0, parent: hit.getParent().name, inTargetHistory: hit.player.getHistory('damage').includes(hit),
      }));
      trials.push({ cancelAt, initial, final: [state(target), state(other)], shaInDiscard: ui.discardPile.contains(sha), hits });
    }
    return { players: game.players.length, trials,
      eventColumns: ['trial','kind','timing','subject','parent','player','source','num','hp','armor','linked','inTargetHistory','inSourceHistory','absorbed'],
      events };
  };
}

export function checkReferenceHp(result) {
  assert.equal(result.players, 8);
  assert.deepEqual(result.trials.map(x => x.cancelAt), ['changeHpBefore', 'changeHpEnd']);
  for (const trial of result.trials) {
    assert.deepEqual(trial.initial, [[4,1,true],[4,0,true]]);
    assert.equal(trial.shaInDiscard, true);
    assert.ok(trial.hits.every(hit => hit.nature === 'fire' && hit.source === 'A' && hit.amount === 2 && hit.inTargetHistory));
  }
  const [before, end] = result.trials;
  assert.deepEqual(before.final, [[4,1,true],[4,0,true]]);
  assert.deepEqual(end.final, [[3,0,false],[2,0,false]]);
  assert.deepEqual(before.hits.map(hit => [hit.target, hit.absorbed, hit.parent]), [['B',0,'sha']]);
  assert.deepEqual(end.hits.map(hit => [hit.target, hit.absorbed, hit.parent]), [['B',1,'sha'],['C',0,'_lianhuan']]);
  for (const { cancelAt } of result.trials) {
    const callbacks = result.events.filter(row => row[0] === cancelAt && row[1] === 'callback' && row[5] === 'B').map(row => row[2]);
    assert.ok(callbacks.includes('changeHpCancelled') && !callbacks.includes('changeHpAfter'));
    for (const timing of ['damage', 'damageSource', 'damageEnd', 'damageAfter']) assert.ok(callbacks.includes(timing));
    assert.ok(!callbacks.includes('damageCancelled'));
  }
}
