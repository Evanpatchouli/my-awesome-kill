import assert from 'node:assert/strict';
export function installShaTargetEquipment() {
  window.__runShaTargetEquipment = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced'); lib.config.game_speed = 'vvfast';
    const seats = [A]; while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1], C = seats[2], D = seats[3];
    A.init('liubei'); B.init('diaochan'); C.init('guanyu');
    for (const p of [A,B,C]) for (const skill of p.getSkills('invisible').slice()) p.removeSkill(skill, true);
    const gender = [A.sex,B.sex,C.sex]; if (gender.join(',') !== 'male,female,male') throw Error(`gender ${gender}`);
    if (game.me !== A) game.swapPlayer(A);
    const label = p => p ? 'ABCDEFGH'[seats.indexOf(p)] ?? null : null;
    const tracked = new Map(), events = [], calls = [], states = [], controls = [], frames = new WeakMap();
    let nextFrame = 0, trial = null, sword;
    const frame = event => { if (!event) return null; if (!frames.has(event)) frames.set(event, ++nextFrame); return frames.get(event); };
    const make = (name, kind, suit, rank) => { const c = game.createCard2(kind, suit, rank); tracked.set(c, name); return c; };
    const ids = cards => (cards || []).flatMap(c => tracked.has(c) ? [tracked.get(c)] : (c.cards || []).filter(x => tracked.has(x)).map(x => tracked.get(x)));
    const zone = c => { const p = get.owner(c), loc = get.position(c, true); return p && loc ? `${label(p)}:${loc}` : loc ?? null; };
    const state = () => ({ hp: [B.hp,C.hp], equips: ids(A.getCards('e')), sword: sword ? zone(sword) : null,
      skills: A.getSkills('invisible').filter(x => x === 'qinggang_skill').length,
      hasSkill: A.hasSkill('qinggang_skill'), ownSkill: A.skills.includes('qinggang_skill'),
      temporary: Object.hasOwn(A.tempSkills, 'qinggang_skill'), equipSkill: A.getSkills('e').includes('qinggang_skill'),
      cards: Object.fromEntries([...tracked].map(([c,id]) => [id,zone(c)])) });
    const timings = ['useCardToPlayered','useCardAfter','chooseControlBegin','chooseControlAfter','judgeBefore','damageEnd'];
    const log = (kind,timing,e) => { if (trial && timings.includes(timing)) events.push([trial,kind,timing,frame(e),e.name,
      frame(e.getParent()),e.getParent()?.name ?? null,label(e.player),label(e.target),ids(e.cards),e.result?.control ?? null,state()]); };
    const observer = 'reference_sha_target_equipment_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true, filter: () => !!trial,
      async content(event, trigger) {
        log('callback',event.triggername,trigger);
        if (trial === 'multi' && event.triggername === 'useCardToPlayered') {
          if (trigger.target === B) await A.discard(sword);
          if (trigger.target === C) { await A.gain(sword); await A.equip(sword); }
        }
      } };
    game.finishSkill(observer); D.addSkill(observer);
    for (const name of ['qinggang_skill','cixiong_skill','bagua_skill']) game.globalEventHandlers.pushHandler(name,(e,{state:step}) => {
      if (trial && ['begin','end'].includes(step)) calls.push([trial,name,step,frame(e),frame(e.getParent()),e.getParent()?.name ?? null,label(e.player),frame(e.getTrigger?.()),e.getTrigger?.()?.name ?? null,state()]);
    });
    const configured = new WeakSet();
    for (const name of ['chooseToUse','chooseBool','chooseControl','chooseToDiscard']) game.globalEventHandlers.pushHandler(name,(e,{state:step}) => {
      if (!trial || step !== 'begin' || configured.has(e)) return; configured.add(e);
      if (name === 'chooseToUse' && e.getParent()?.name === 'phaseUse') { e.ai1 = c => tracked.get(c) === 'multi-sha' ? 20 : -10; e.ai2 = p => p === B || p === C ? 10 : -10; }
      if (name === 'chooseBool') { if (trial === 'human-order' && !_status.auto) ui.click.auto('forced'); e.ai = () => true; }
      if (name === 'chooseToDiscard') { if (!_status.auto) ui.click.auto('forced'); e.ai = () => 10; }
      if (name === 'chooseControl') { controls.push([trial,e.getParent()?.name,e.controls?.slice(),e.player.isMine(),_status.auto]);
        if (trial !== 'human-order') e.ai = () => 0; }
    });
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function(timing) { log('request',timing,this); return original.apply(this,arguments); };
    try {
      for (const p of seats) { const held=p.getCards('hej'); if (held.length) await p.discard(held); }
      B.hp=B.maxHp=12; C.hp=C.maxHp=12; B.update(); C.update();
      sword = make('sword','qinggang','spade',6); const fang = make('fang','fangtian','spade',12);
      const armorB=make('armor-B','bagua','club',2), armorC=make('armor-C','bagua','club',3);
      await A.expandEquip(1); await A.equip(fang); await A.equip(sword); await B.equip(armorB); await C.equip(armorC);
      const sha=make('multi-sha','sha','spade',7), red=make('red','sha','heart',9); await A.gain(sha); ui.cardPile.prepend(red);
      trial='multi'; await A.phaseUse(); states.push(['multi',state()]); trial=null;
      trial='virtual'; await A.useCard({name:'sha',isCard:true}, B, false); states.push(['virtual',state()]); trial=null;
      for (const p of [A,B,C]) { const held=p.getCards('hej'); if (held.length) await p.discard(held); }
      B.hp=B.maxHp=12; B.update(); await A.equip(sword); await B.equip(armorB);
      const outer=make('nested-outer','sha','spade',8), inner=make('nested-inner','sha','spade',9), nestedRed=make('nested-red','sha','heart',10);
      await A.gain(outer); await C.gain(inner); ui.cardPile.prepend(nestedRed);
      const nestedSkill='reference_nested_sha_target';
      lib.skill[nestedSkill] = { trigger:{global:'useCardToPlayered'}, lastDo:true, forced:true, silent:true,
        filter:e => trial === 'nested' && e.player === A && e.target === B && e.cards?.includes(outer),
        async content() { await C.useCard(inner,B,false); } };
      game.finishSkill(nestedSkill); D.addSkill(nestedSkill);
      trial='nested'; await A.useCard(outer,B,false); states.push(['nested',state()]); trial=null;
      D.removeSkill(nestedSkill);
      for (const p of [A,B,C]) { const held=p.getCards('hej'); if (held.length) await p.discard(held); }
      B.hp=B.maxHp=12; B.update();
      const twin=make('twin','cixiong','spade',2); sword=make('order-sword','qinggang','spade',6);
      await A.equip(twin); await A.equip(sword);
      const humanSha=make('human-sha','sha','spade',8), humanCost=make('human-cost','tao','heart',4);
      await A.gain(humanSha); await B.gain(humanCost);
      trial='human-order'; if (_status.auto) ui.click.auto('forced');
      await A.useCard(humanSha,B,false); states.push(['human-order',state()]); trial=null;
      for (const p of [A,B]) { const held=p.getCards('hej'); if (held.length) await p.discard(held); }
      const use = async name => { const card=make(`${name}-sha`,'sha','spade',9); await A.gain(card);
        trial=name; await A.useCard(card,B,false); states.push([name,state()]); trial=null; };
      A.addTempSkill('qinggang_skill'); await A.equip(twin); await A.equip(sword);
      states.push(['probe-before-remove',state()]); A.removeSkill('qinggang_skill');
      states.push(['probe-after-remove',state()]); await use('temporary-removed-while-equipped');
      for (const p of [A,B]) { const held=p.getCards('hej'); if (held.length) await p.discard(held); }
      A.addTempSkill('qinggang_skill'); states.push(['temporary-before-equip',state()]);
      await A.equip(twin); await A.equip(sword); states.push(['temporary-with-equipment',state()]);
      await use('temporary-plus-equipped');
      await A.discard(sword); states.push(['temporary-after-discard',state()]);
      await use('temporary-after-discard');
      A.removeSkill('qinggang_skill'); states.push(['temporary-removed',state()]);
      await A.equip(sword); await use('equipment-after-temporary');
      for (const p of [A,B]) { const held=p.getCards('hej'); if (held.length) await p.discard(held); }
      await A.equip(twin); await A.equip(sword); states.push(['equipment-before-temporary',state()]);
      A.addTempSkill('qinggang_skill'); states.push(['equipment-then-temporary',state()]);
      await use('equipment-then-temporary');
      A.removeSkill('qinggang_skill'); states.push(['remove-temporary-with-equipment',state()]);
      await use('remove-temporary-with-equipment');
      return { gender, events, calls, states, controls };
    } finally { proto.trigger=original; D.removeSkill(observer); }
  };
}
export function checkShaTargetEquipment(result) {
  assert.deepEqual(result.gender, ['male', 'female', 'male']);
  assert.equal(result.humanCixiongClick, true);
  const rows = (trial, timing, kind = 'request') => result.events.filter(row => row[0] === trial && row[1] === kind && row[2] === timing);
  const stage = name => result.states.find(row => row[0] === name)[1];
  const skills = trial => result.calls.filter(row => row[0] === trial && row[2] === 'begin').map(row => row[1]);
  const targets = rows('multi', 'useCardToPlayered');
  assert.equal(targets.length, 2);
  assert.notEqual(targets[0][3], targets[1][3]);
  assert.equal(targets[0][5], targets[1][5]);
  assert.deepEqual(targets.map(row => [row[4], row[6], row[7], row[8], row[9]]), [
    ['useCardToPlayered', 'useCard', 'A', 'B', ['multi-sha']],
    ['useCardToPlayered', 'useCard', 'A', 'C', ['multi-sha']],
  ]);
  assert.deepEqual(rows('virtual', 'useCardToPlayered').map(row => [row[6], row[8], row[9]]), [['useCard', 'B', []]]);
  assert.deepEqual(stage('multi').hp, [12, 11]);
  assert.deepEqual(stage('nested').hp, [10, 11]);
  assert.equal(stage('nested').cards['nested-red'], 'c');
  assert.equal(skills('nested').filter(skill => skill === 'bagua_skill').length, 0);
  assert.deepEqual(rows('nested', 'useCardToPlayered').map(row => [row[7], row[8], row[9]]), [['A', 'B', ['nested-outer']], ['C', 'B', ['nested-inner']]]);
  assert.equal(stage('multi').cards.red, 'd');
  assert.deepEqual(skills('multi'), ['qinggang_skill', 'bagua_skill']);
  assert.deepEqual(skills('human-order'), ['cixiong_skill', 'qinggang_skill']);
  assert.deepEqual(result.controls.find(row => row[0] === 'human-order').slice(3), [true, false]);
  assert.deepEqual(rows('human-order', 'chooseControlAfter').map(row => row[10]), ['雌雄双股剑']);
  assert.deepEqual(skills('temporary-plus-equipped'), ['qinggang_skill', 'cixiong_skill']);
  assert.deepEqual(skills('temporary-after-discard'), ['qinggang_skill', 'cixiong_skill']);
  assert.deepEqual(skills('equipment-after-temporary'), ['cixiong_skill', 'qinggang_skill']);
  assert.deepEqual(skills('equipment-then-temporary'), ['cixiong_skill', 'qinggang_skill']);
  assert.deepEqual([stage('equipment-before-temporary').temporary, stage('equipment-then-temporary').temporary], [false, false]);
  assert.deepEqual([stage('probe-before-remove').temporary, stage('probe-after-remove').temporary,
    stage('probe-after-remove').equipSkill, stage('probe-after-remove').sword], [true, false, true, 'A:e']);
  assert.deepEqual(skills('temporary-removed-while-equipped'), ['cixiong_skill']);
  for (const row of result.calls.filter(row => row[2] === 'begin' && row[1] !== 'bagua_skill')) {
    assert.ok(row[3] && row[4] && row[7]);
    assert.equal(row[8], 'useCardToPlayered');
  }
}
