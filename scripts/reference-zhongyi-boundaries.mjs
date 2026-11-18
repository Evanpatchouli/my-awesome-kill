import assert from 'node:assert/strict';

export function installZhongyiBoundaries() {
  const events = [];
  const trials = [];
  let seats;
  let sample;
  let firstRound;
  let expiry;
  let restore;

  window.__runZhongyiBoundaries = async ({ lib, game, ui, get, _status, actor }) => {
    const seat = player => Object.keys(seats).find(name => seats[name] === player) ?? (player ? 'other' : null);
    const snapshot = ({ owner, card }) => {
      const holder = get.owner(card);
      const position = get.position(card, true) || null;
      return {
        zone: holder ? `${seat(holder)}:${position}` : position,
        inSpecial: ui.special.contains(card), knownToSpectator: card.isKnownBy(game.me),
        tagged: owner.getExpansions('zhongyi2').includes(card), mark: !!owner.marks.zhongyi2,
        buff: owner.hasSkill('zhongyi2'), spent: owner.awakenedSkills.includes('zhongyi'),
        disabled: !!owner.disabledSkills.zhongyi?.includes('zhongyi_awake'),
      };
    };
    const markShows = ({ owner, card }) => {
      const mark = owner.marks.zhongyi2;
      if (!mark) return false;
      const dialog = get.nodeintro(mark);
      const exposed = dialog.buttons.some(button => button.link === card && !button.classList.contains('infohidden'));
      dialog.close();
      return exposed;
    };

    async function strike(source, targets, { virtualFire = false } = {}) {
      for (const target of targets) {
        const cards = target.getCards('h', card => card !== trials[0]?.card);
        if (cards.length) await target.discard(cards);
        target.hp = target.maxHp;
        target.update();
        if (virtualFire) await target.link(true);
      }
      const card = virtualFire ? { name: 'sha', nature: 'fire', isCard: true } : game.createCard2('sha', 'spade', 7);
      if (!virtualFire) await source.gain(card);
      const before = targets.map(target => target.hp);
      const use = source.useCard(card, targets[0], false);
      await use;
      return targets.map((target, index) => {
        const damage = target.getHistory('damage').at(-1);
        if (!damage || damage.source !== source || target.hp === before[index]) throw new Error('Expected real Sha damage');
        return { source: seat(source), target: seat(target), amount: before[index] - target.hp,
          parent: damage.getParent().name, physicalCards: damage.cards?.length ?? 0 };
      });
    }

    if (seats) {
      if (!expiry?.callback) return;
      expiry.attack = await strike(seats.A, [seats.B]);
      expiry.final = snapshot(sample);
      restore();
      return { playersAtStart: Object.keys(seats).length, playersAfterDeath: game.players.length,
        trials: trials.map(({ row }) => row), expiry, events };
    }

    const ordered = [actor];
    for (let player = actor.next; player !== actor; player = player.next) ordered.push(player);
    if (ordered.slice(0, 6).includes(game.me) || ordered[4] === game.zhu) {
      if (actor === game.me && actor.countCards('h')) await actor.discard(actor.getCards('h'));
      return;
    }
    seats = Object.fromEntries(ordered.map((player, index) => ['ABCDEFGH'[index], player]));
    const { A, B, C, D, E, F } = seats;
    firstRound = game.roundNumber;
    const skills = (await import('/character/standard/skill.js')).default;
    for (const name of ['zhongyi', 'zhongyi2']) {
      if (!lib.skill[name]) {
        lib.skill[name] = skills[name];
        game.finishSkill(name);
      }
    }
    game.showIdentity();
    for (const player of ordered) {
      const held = player.getCards('he');
      if (held.length) await player.discard(held);
      player.side = [A, B].includes(player);
      player.hp = player.maxHp = 8;
      player.hujia = 0;
      player.update();
    }

    function capture(kind, timing, event) {
      events.push({ sample: sample.row.name, kind, timing, event: event.name,
        parent: event.getParent()?.name || null, player: seat(event.player), ...snapshot(sample) });
    }
    const proto = lib.element.GameEvent.prototype;
    const originalTrigger = proto.trigger;
    const originalCheck = lib.skill.zhongyi.check;
    restore = () => {
      proto.trigger = originalTrigger;
      if (originalCheck === undefined) delete lib.skill.zhongyi.check;
      else lib.skill.zhongyi.check = originalCheck;
    };
    // Prefer the equipped card through the normal AI material scoring path.
    lib.skill.zhongyi.check = material => material === sample?.card ? 10 : -10;
    const timings = new Set(['useSkillBegin', 'useSkillAfter', 'addToExpansionBefore', 'addToExpansionAfter',
      'loseBefore', 'loseAfter', 'gainAfter', 'loseToDiscardpileBefore', 'loseToDiscardpileAfter', 'die', 'dieAfter']);
    proto.trigger = function (timing) {
      if (sample) {
        if (timings.has(timing) && (this.cards?.includes(sample.card) ||
            (this.player === sample.owner && ['die', 'dieAfter'].includes(timing)))) capture('request', timing, this);
        if (timing === 'roundStart' && game.roundNumber > firstRound && !expiry.request) {
          expiry.request = snapshot(sample);
          capture('request', timing, this);
        }
      }
      return originalTrigger.apply(this, arguments);
    };
    const observer = 'reference_zhongyi_boundary';
    lib.skill[observer] = {
      trigger: { global: ['addToExpansionBefore', 'loseBefore', 'loseAfter', 'roundStart', 'die', 'dieAfter'] },
      forced: true, silent: true, firstDo: true, forceDie: true,
      filter(event, player, timing) {
        if (!sample) return false;
        if (timing === 'roundStart') return game.roundNumber > firstRound && !expiry.callback;
        if (['die', 'dieAfter'].includes(timing)) return event.player === sample.owner;
        return event.cards?.includes(sample.card) &&
          (event.name === 'addToExpansion' || event.getParent()?.name === 'addToExpansion');
      },
      async content(event, trigger) {
        const timing = event.triggername;
        capture('callback', timing, trigger);
        if (timing === 'roundStart') expiry.callback = snapshot(sample);
        if ((timing === 'addToExpansionBefore' && sample.row.name === 'cancel-expansion') ||
            (timing === 'loseBefore' && sample.row.name === 'cancel-lose')) trigger.cancel();
      },
    };
    game.finishSkill(observer);
    A.addSkill(observer);
    game.globalEventHandlers.pushHandler('zhongyi2', (event, { state }) => {
      if (!sample || state !== 'end') return;
      const damage = event.getTrigger();
      events.push({ sample: sample.row.name, kind: 'content', timing: state, event: event.name,
        holder: seat(event.player), source: seat(damage.source), target: seat(damage.player), amount: damage.num });
    });

    async function prepare(name, owner, card, location = 'h') {
      sample = undefined;
      if (location === 'e') await owner.equip(card);
      else {
        ui.cardPile.prepend(card);
        await owner.draw();
      }
      owner.addSkill('zhongyi');
      sample = { owner, card, row: { name, owner: seat(owner), material: [card.name, get.color(card), get.number(card)] } };
      sample.row.before = snapshot(sample);
      trials.push(sample);
    }

    async function direct() {
      await sample.owner.useSkill({ skill: 'zhongyi', cards: [sample.card], targets: [] });
      sample.row.activated = snapshot(sample);
      sample.row.markShowsMaterial = markShows(sample);
    }

    await prepare('move-away', A, game.createCard2('qinggang', 'spade', 6), 'e');
    const fallback = game.createCard2('tao', 'heart', 3);
    await A.gain(fallback);
    const activeChoice = async () => {
      let offered = false;
      const result = await A.chooseToUse().set('type', 'phase').set('ai1', option => {
        if (option !== 'zhongyi') return -10;
        offered = true;
        return 10;
      }).set('ai2', () => 10).forResult();
      return { offered, accepted: result.bool, skill: result.skill || null,
        selected: result.cards?.map(card => card === sample.card ? 'material' : 'other') || [] };
    };
    sample.row.choices = [await activeChoice(), await activeChoice()];
    sample.row.activated = snapshot(sample);
    sample.row.markShowsMaterial = markShows(sample);
    await B.gain(sample.card, A, 'give');
    sample.row.diverted = snapshot(sample);
    sample.row.attack = await strike(A, [B, C], { virtualFire: true });
    window.__referenceStages.push('move-away: resolved');

    await prepare('cancel-expansion', C, game.createCard2('shan', 'club', 2));
    await direct();
    sample.row.attack = await strike(C, [D]);
    C.removeSkill('zhongyi2');
    window.__referenceStages.push('cancel-expansion: resolved');

    await prepare('cancel-lose', D, game.createCard2('shan', 'spade', 2));
    await direct();
    sample.row.attack = await strike(D, [C]);
    window.__referenceStages.push('cancel-lose: resolved');

    await prepare('holder-death', E, game.createCard2('tao', 'heart', 7));
    await direct();
    sample.row.stacked = await strike(F, [E]);
    sample.row.currentTurn = seat(_status.currentPhase);
    await E.die().set('animate', false);
    sample.row.dead = E.isDead();
    sample.row.afterDeath = snapshot(sample);
    sample.row.afterDeathAttack = await strike(F, [D]);
    window.__referenceStages.push('holder-death: resolved');

    D.removeSkill('zhongyi2');
    sample = trials[0];
    expiry = {};
  };
}

export function checkZhongyiBoundaries(result) {
  assert.equal(result.playersAtStart, 8);
  assert.equal(result.playersAfterDeath, 7);
  assert.deepEqual(result.trials.map(trial => trial.name), ['move-away', 'cancel-expansion', 'cancel-lose', 'holder-death']);
  const [moved, outer, inner, death] = result.trials;
  const initial = { inSpecial: false, knownToSpectator: false, tagged: false, mark: false, buff: false, spent: false, disabled: false };
  const active = { ...initial, buff: true, spent: true, disabled: true };
  const expanded = { ...active, tagged: true, mark: true };
  const inactive = { ...active, buff: false };
  const state = row => Object.fromEntries(['zone', ...Object.keys(initial)].map(key => [key, row[key]]));
  const rows = trial => result.events.filter(event => event.sample === trial.name);
  const event = (trial, kind, timing, parent) => rows(trial).find(row => row.kind === kind && row.timing === timing &&
    (parent === undefined || row.parent === parent));
  const damage = attacks => attacks.map(hit => [hit.amount, hit.parent, hit.physicalCards]);

  assert.deepEqual(moved.material, ['qinggang', 'black', 6]);
  assert.deepEqual(moved.choices, [
    { offered: true, accepted: true, skill: 'zhongyi', selected: ['material'] },
    { offered: false, accepted: false, skill: null, selected: [] },
  ]);
  assert.deepEqual(state(event(moved, 'request', 'useSkillBegin', 'chooseToUse')), { ...initial, zone: 'A:e', knownToSpectator: true });
  const revealedTransit = { ...active, zone: 's', inSpecial: true, knownToSpectator: true };
  assert.deepEqual(state(event(moved, 'callback', 'loseAfter', 'addToExpansion')), revealedTransit);
  assert.deepEqual(moved.activated, { ...expanded, zone: 'A:x', knownToSpectator: true });
  assert.equal(moved.markShowsMaterial, true);
  assert.deepEqual(state(event(moved, 'request', 'loseAfter', 'gain')), revealedTransit);
  assert.deepEqual(moved.diverted, { ...active, zone: 'B:h', knownToSpectator: true });
  assert.deepEqual(damage(moved.attack), [[2, 'sha', 0], [2, '_lianhuan', 0]]);
  assert.deepEqual(rows(moved).filter(row => row.event === 'zhongyi2').map(row => row.target), ['B']);
  assert.deepEqual(result.expiry.request, moved.diverted);
  assert.deepEqual(result.expiry.callback, { ...moved.diverted, buff: false });
  assert.deepEqual(result.expiry.final, result.expiry.callback);
  assert.deepEqual(damage(result.expiry.attack), [[1, 'sha', 1]]);
  assert.ok(!rows(moved).some(row => row.event === 'loseToDiscardpile'));

  for (const [trial, owner, known] of [[outer, 'C', false], [inner, 'D', true]]) {
    assert.deepEqual(trial.before, { ...initial, zone: `${owner}:h` });
    assert.deepEqual(trial.activated, { ...active, zone: `${owner}:h`, knownToSpectator: known });
    assert.equal(trial.markShowsMaterial, false);
    assert.deepEqual(damage(trial.attack), [[2, 'sha', 1]]);
    assert.ok(!rows(trial).some(row => row.timing === 'loseAfter' && row.parent === 'addToExpansion'));
  }
  assert.equal(event(outer, 'callback', 'addToExpansionBefore').knownToSpectator, false);
  assert.equal(event(outer, 'request', 'loseBefore'), undefined);
  assert.equal(event(inner, 'callback', 'loseBefore', 'addToExpansion').knownToSpectator, true);

  assert.deepEqual(death.before, { ...initial, zone: 'E:h' });
  assert.deepEqual(state(event(death, 'callback', 'loseAfter', 'addToExpansion')), { ...active, zone: 's', inSpecial: true });
  assert.deepEqual(death.activated, { ...expanded, zone: 'E:x' });
  assert.equal(death.markShowsMaterial, true);
  assert.deepEqual(damage(death.stacked), [[3, 'sha', 1]]);
  assert.equal(death.currentTurn, 'A');
  assert.equal(death.dead, true);
  assert.deepEqual(state(event(death, 'callback', 'die')), death.activated);
  assert.deepEqual(state(event(death, 'callback', 'dieAfter')), { ...inactive, zone: 'd' });
  assert.deepEqual(death.afterDeath, state(event(death, 'callback', 'dieAfter')));
  assert.deepEqual(rows(death).filter(row => row.timing === 'loseAfter' && row.zone === 'd').map(row => row.parent), ['discard']);
  assert.deepEqual(damage(death.afterDeathAttack), [[2, 'sha', 1]]);
  assert.deepEqual(rows(death).filter(row => row.event === 'zhongyi2').map(row => [row.holder, row.target, row.amount]), [
    ['D', 'E', 2], ['E', 'E', 3], ['D', 'D', 2],
  ]);
}
