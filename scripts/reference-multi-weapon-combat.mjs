import assert from 'node:assert/strict';

export function installMultiWeaponCombat() {
  window.__runMultiWeaponCombat = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const seats = [A]; while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1]; if (B === game.me) game.swapPlayer(A);
    const seat = p => p ? 'ABCDEFGH'[seats.indexOf(p)] ?? null : null;
    const tracked = new Map(), rows = [], calls = [], stages = [], controls = [];
    let trial = null, phaseUses = 0;
    const make = (id, name, suit, rank) => { const card = game.createCard2(name, suit, rank); tracked.set(card, id); return card; };
    const ids = cards => (cards || []).flatMap(c => tracked.has(c) ? [tracked.get(c)] : (c.cards || []).filter(x => tracked.has(x)).map(x => tracked.get(x)));
    const zone = c => { const owner = get.owner(c), pos = get.position(c, true); return owner && pos ? `${seat(owner)}:${pos}` : pos ?? null; };
    const state = () => ({ hp: B.hp, distance: A.getAttackRange(), slots: A.countEnabledSlot(1), equipment: ids(A.getCards('e')),
      skills: A.getSkills('e').filter(x => x === 'qinggang_skill'), cards: Object.fromEntries([...tracked].map(([c, id]) => [id, zone(c)])) });
    const timings = ['useCardBefore', 'useCardAfter', 'useCardToPlayered', 'shaMiss', 'shaHit', 'damageBegin1', 'damageEnd',
      'judgeBefore', 'judgeAfter', 'chooseToUseBegin', 'chooseToUseAfter', 'chooseToRespondBegin', 'chooseToRespondAfter',
      'chooseControlBegin', 'chooseControlAfter', 'chooseToDiscardBegin', 'chooseToDiscardAfter'];
    const record = (type, timing, event) => { if (trial && timings.includes(timing)) rows.push([trial, type, timing, event.name,
      event.getParent()?.name ?? null, seat(event.player), seat(event.target), ids(event.cards), event.result?.control ?? null, state()]); };
    const observer = 'reference_multi_weapon_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true, filter: () => !!trial,
      async content(event, trigger) {
        record('callback', event.triggername, trigger);
        if (trial === 'phase' && event.triggername === 'useCardAfter' && trigger.player === A && trigger.card?.name === 'sha') {
          phaseUses++;
          if (phaseUses === 1) await A.discard(sword1);
          if (phaseUses === 2) await A.discard(crossbow);
        }
      } };
    game.finishSkill(observer); seats[2].addSkill(observer);
    for (const skill of ['qinggang_skill', 'bagua_skill', 'qinglong_skill', 'guanshi_skill']) {
      game.globalEventHandlers.pushHandler(skill, (event, { state: step }) => {
        if (trial && ['begin', 'end'].includes(step)) calls.push([trial, skill, step, seat(event.player), event.getParent()?.name ?? null, state()]);
      });
    }
    const configured = new WeakSet();
    for (const name of ['chooseToUse', 'chooseToRespond', 'chooseBool', 'chooseToDiscard', 'chooseControl']) {
      game.globalEventHandlers.pushHandler(name, (event, { state: step }) => {
        if (!trial || step !== 'begin' || configured.has(event)) return;
        configured.add(event);
        if (name === 'chooseControl') { controls.push([trial, event.getParent()?.name ?? null, event.controls?.slice(), event.player.isMine(), _status.auto]); if (trial !== 'axe-first') event.ai = () => event.controls.findIndex(item => String(item).includes('青龙偃月刀')); }
        else if (name === 'chooseToUse' && event.getParent()?.name === 'phaseUse') {
          event.ai1 = card => tracked.get(card)?.startsWith('phase-sha') ? 20 - get.number(card) : -10;
          event.ai2 = target => target === B ? 10 : -10;
        } else if (name === 'chooseToUse' && event.getParent()?.name === 'qinglong_skill') {
          event.ai1 = card => tracked.get(card)?.endsWith('follow-sha') ? 20 : -10;
          event.ai2 = target => target === B ? 10 : -10;
        } else if (name === 'chooseToRespond') event.ai = card => tracked.get(card)?.endsWith('shan') ? 20 : -10;
        else if (name === 'chooseBool') event.ai = () => true;
        else if (name === 'chooseToDiscard') { if (trial === 'axe-first' && !_status.auto) ui.click.auto('forced'); event.ai = card => tracked.get(card)?.includes('cost') ? 20 : -10; }
      });
    }
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function(timing) { record('request', timing, this); return original.apply(this, arguments); };
    let crossbow, sword1;
    try {
      for (const p of seats) { const held = p.getCards('hej'); if (held.length) await p.discard(held); }
      B.hp = B.maxHp = 12; B.update();
      crossbow = make('crossbow', 'zhuge', 'club', 1); sword1 = make('sword-1', 'qinggang', 'spade', 6);
      const sword2 = make('sword-2', 'qinggang', 'spade', 7), armor = make('armor', 'bagua', 'spade', 2);
      await A.equip(crossbow); await B.equip(armor);
      const control = make('control-sha', 'sha', 'spade', 8), red = make('red', 'sha', 'heart', 9);
      await A.gain(control); ui.cardPile.prepend(red);
      trial = 'control'; stages.push({ name: 'control-before', ...state() }); await A.useCard(control, B, false); stages.push({ name: 'control-after', ...state() });
      trial = null; await A.expandEquip([1,1]); await A.equip(sword1); await A.equip(sword2);
      stages.push({ name: 'armed', ...state() });
      const attacks = [1,2,3].map(n => make(`phase-sha-${n}`, 'sha', 'spade', n)); await A.gain(attacks);
      trial = 'phase'; await A.phaseUse(); stages.push({ name: 'phase-after', ...state() }); trial = null;
      await A.disableEquip(1);
      for (const name of ['axe-first', 'dragon-first']) {
        for (const p of [A, B]) { const held = p.getCards('hej'); if (held.length) await p.discard(held); }
        B.hp = B.maxHp = 12; B.update();
        const dragon = make(`${name}-dragon`, 'qinglong', 'spade', 5);
        const axe = make(`${name}-axe`, 'guanshi', 'diamond', 5);
        const sha = make(`${name}-sha`, 'sha', 'spade', 10);
        const follow = make(`${name}-follow-sha`, 'sha', 'spade', 11);
        const shan = make(`${name}-shan`, 'shan', 'heart', 2);
        const costs = [1,2].map(n => make(`${name}-cost-${n}`, 'tao', 'heart', n));
        await A.equip(dragon); await A.equip(axe); await A.gain([sha, follow, ...costs]); await B.gain(shan);
        trial = name; if (name === 'axe-first') { if (game.me !== A) game.swapPlayer(A); if (_status.auto) ui.click.auto('forced'); }
        stages.push({ name: `${name}-before`, ...state() });
        await A.useCard(sha, B, false);
        stages.push({ name: `${name}-after`, ...state() }); trial = null;
      }
      return { columns: ['trial','kind','timing','event','parent','player','target','cards','control','state'], events: rows, calls, stages, controls };
    } finally { proto.trigger = original; seats[2].removeSkill(observer); }
  };
}
export function checkMultiWeaponCombat(result) {
  const stage = name => result.stages.find(item => item.name === name);
  const requests = (trial, timing) => result.events.filter(row => row[0] === trial && row[1] === 'request' && row[2] === timing);
  const calls = (trial, skill) => result.calls.filter(row => row[0] === trial && row[1] === skill && row[2] === 'begin');
  assert.deepEqual([stage('control-before').distance, stage('control-after').hp, stage('control-after').cards.red], [1, 12, 'd']);
  assert.equal(calls('control', 'bagua_skill').length, 1);
  assert.equal(requests('control', 'judgeBefore').length, 1);
  assert.deepEqual([stage('armed').distance, stage('armed').skills, stage('armed').equipment],
    [2, ['qinggang_skill'], ['sword-2', 'sword-1', 'crossbow']]);
  const phase = stage('phase-after');
  assert.deepEqual([phase.hp, phase.cards['phase-sha-1'], phase.cards['phase-sha-2'], phase.cards['phase-sha-3'],
    phase.cards['sword-1'], phase.cards['sword-2'], phase.cards.crossbow], [10, 'd', 'd', 'A:h', 'd', 'A:e', 'd']);
  assert.equal(calls('phase', 'qinggang_skill').length, 2);
  assert.equal(calls('phase', 'bagua_skill').length, 0);
  assert.equal(requests('phase', 'judgeBefore').length, 0);
  assert.deepEqual(requests('phase', 'useCardBefore').map(row => row[7]), [['phase-sha-1'], ['phase-sha-2']]);
  for (const trial of ['axe-first', 'dragon-first']) {
    const before = stage(`${trial}-before`);
    assert.equal(before.slots, 2);
    assert.equal(before.equipment.length, 2);
    const after = stage(`${trial}-after`);
    assert.deepEqual([after.cards[`${trial}-sha`], after.cards[`${trial}-shan`], after.cards[`${trial}-cost-1`],
      after.cards[`${trial}-cost-2`]], ['d', 'd', 'd', 'd']);
    assert.deepEqual(requests(trial, 'chooseControlAfter').map(row => row[8]), [trial === 'axe-first' ? '贯石斧' : '青龙偃月刀']);
    assert.equal(calls(trial, 'guanshi_skill').length, 1);
  }
  assert.deepEqual(result.controls.find(row => row[0] === 'axe-first').slice(3), [true, false]);
  assert.equal(result.humanAxeClick, true);
  assert.equal(stage('axe-first-after').hp, 11);
  assert.equal(stage('axe-first-after').cards['axe-first-follow-sha'], 'A:h');
  assert.equal(calls('axe-first', 'qinglong_skill').length, 0);
  assert.deepEqual(requests('axe-first', 'damageEnd').map(row => row[7]), [['axe-first-sha']]);
  assert.equal(stage('dragon-first-after').hp, 10);
  assert.equal(stage('dragon-first-after').cards['dragon-first-follow-sha'], 'd');
  assert.equal(calls('dragon-first', 'qinglong_skill').length, 1);
  assert.deepEqual(requests('dragon-first', 'damageEnd').map(row => row[7]), [['dragon-first-follow-sha'], ['dragon-first-sha']]);
  for (const trial of ['control', 'phase', 'axe-first', 'dragon-first']) {
    assert.deepEqual(result.events.filter(row => row[0] === trial && row[1] === 'request').map(row => row[2]),
      result.events.filter(row => row[0] === trial && row[1] === 'callback').map(row => row[2]), trial);
  }
}
