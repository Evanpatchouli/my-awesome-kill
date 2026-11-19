import assert from 'node:assert/strict';

export function installVirtualEquipCombat() {
  window.__runVirtualEquipCombat = async ({ lib, game, ui, get, _status, actor }) => {
    if (!_status.auto) ui.click.auto('forced');
    // Use a non-lord source so its later ordinary death does not end identity mode.
    const seats = [actor.nextSeat];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const A = seats[0], B = seats[1], C = seats[2], label = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    if (A === game.zhu) throw new Error('Expected a non-lord attacker');
    if (A === game.me || B === game.me) game.swapPlayer(game.zhu);
    const timings = ['useCardBefore', 'useCardToPlayered', 'useCardAfter', 'shaBefore', 'shaBegin',
      'shaCancelled', 'shaMiss', 'shaEnd', 'damageBefore', 'damageBegin1', 'damage', 'damageEnd', 'damageAfter',
      'chooseToRespondBefore', 'chooseToRespondBegin', 'chooseToRespondAfter', 'chooseToUseBefore', 'chooseToUseBegin',
      'chooseToUseAfter', 'judgeBefore', 'judge', 'judgeEnd', 'judgeAfter', 'discardBefore',
      'dieBefore', 'dieBegin', 'die', 'dieEnd', 'dieAfter'];
    let trial = null, sha = null, weapon = null, red = null, setup = false;
    const events = [], trials = [], skillCalls = [], eligibility = [];
    const zone = card => { const pos = get.position(card, true), owner = get.owner(card); return owner && pos ? `${label(owner)}:${pos}` : pos ?? null; };
    const state = () => ({ targetHp: B.hp, sha: sha ? zone(sha) : null, weapon: weapon ? zone(weapon) : null,
      armor: B.getEquip('renwang') ? zone(B.getEquip('renwang')) : B.getEquip('bagua') ? zone(B.getEquip('bagua')) : null,
      armorName: B.getEquip('renwang') ? 'renwang' : B.getEquip('bagua') ? 'bagua' : null,
      red: red ? zone(red) : null, targetShan: B.countCards('h', 'shan'),
      attackerPhysical: A.countCards('e'), attackerVirtual: !!A.getVEquip('qinggang'),
      attackerRange: A.getEquipRange(), qinggang: A.getSkills('e').includes('qinggang_skill'),
      armorSkill: B.getSkills('e').includes('renwang_skill') || B.getSkills('e').includes('bagua_skill'),
      armorSuppressed: B.hasSkill('qinggang2') });
    function record(kind, timing, event) {
      if (trial && timings.includes(timing)) events.push([trial, kind, timing, event.name,
        event.getParent()?.name ?? null, label(event.player), label(event.source), label(event.target), state()]);
    }
    const observer = 'reference_virtual_combat_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true,
      filter: () => !!trial || setup,
      async content(event, trigger) {
        record('callback', event.triggername, trigger);
        if (trial?.startsWith('bagua') && ['chooseToRespondBegin', 'chooseToUseBegin'].includes(event.triggername) && trigger.player === B) {
          eligibility.push({ trial, timing: event.triggername, eligible: lib.skill.bagua_skill.filter(trigger, B), state: state() });
        }
        if (setup && event.triggername === 'discardBefore' && trigger.player === A && trigger.getParent('disableJudge', true)) {
          setup = false;
          await A.gain(weapon);
          await A.equip(weapon);
        }
      } };
    game.finishSkill(observer); C.addSkill(observer);
    for (const skill of ['qinggang_skill', 'renwang_skill', 'bagua_skill', 'jiedao']) {
      game.globalEventHandlers.pushHandler(skill, (event, { state: timing }) => {
        if (trial && ['begin', 'end'].includes(timing)) skillCalls.push([trial, skill, timing, label(event.player), state()]);
      });
    }
    game.globalEventHandlers.pushHandler('chooseBool', (event, { state: timing }) => {
      if (trial?.startsWith('bagua') && event.player === B && timing === 'begin') event.ai = () => true;
    });
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return original.apply(this, arguments); };
    try {
      for (const player of seats) {
        const cards = player.getCards('hej'); if (cards.length) await player.discard(cards);
      }
      const armor = game.createCard2('renwang', 'club', 2);
      await B.equip(armor);
      B.hp = B.maxHp = 4; B.update();
      for (const name of ['control', 'orphan', 'bagua-control', 'bagua-orphan']) {
        if (name === 'bagua-control') {
          A.removeVirtualEquip(A.getVEquip('qinggang'));
          await A.enableJudge();
          weapon = null;
          await B.equip(game.createCard2('bagua', 'club', 2));
          B.hp = B.maxHp = 4; B.update();
        }
        if (name === 'orphan' || name === 'bagua-orphan') {
          weapon = game.createCard2('qinggang', 'diamond', 6);
          await A.addJudge({ name: 'lebu' }, [weapon]);
          setup = true;
          await A.disableJudge();
          if (setup) throw new Error('Failed to create the virtual equipment residue');
        }
        sha = game.createCard2('sha', 'spade', 7);
        await A.gain(sha);
        if (name.startsWith('bagua')) {
          red = game.createCard2('sha', 'heart', 9);
          ui.cardPile.prepend(red);
        }
        trial = name;
        const before = state();
        await A.useCard(sha, B, false);
        trials.push({ name, before, after: state() });
        trial = null;
      }
      // Actual jiedao after the orphaned weapon's last attack; no cards in A's
      // hand and no physical equipment are fabricated for the fallback.
      const trick = game.createCard2('jiedao', 'club', 12);
      await C.gain(trick);
      const eligibilityChecks = { targetHand: A.countCards('h'), targetPhysicalEquip: A.countCards('e'),
        virtualWeaponMaterials: A.getEquips(1).map(zone),
        canTargetJiedao: lib.card.jiedao.filterTarget(trick, C, A),
        canTargetGuohe: lib.card.guohe.filterTarget({ name: 'guohe' }, C, A),
        beforeWeapon: zone(weapon), sourceHandBefore: C.countCards('h') };
      trial = 'jiedao';
      const use = C.useCard(trick, A, false);
      use.addedTarget = B;
      use.addedTargets = [B];
      await use;
      const jiedao = { ...eligibilityChecks, trick: zone(trick), afterWeapon: zone(weapon),
        sourceHasWeapon: C.getCards('h').includes(weapon), targetHandAfter: A.countCards('h'), sourceHandAfter: C.countCards('h') };
      trial = null;
      const deathState = () => ({ alive: A.isAlive(), out: A.isOut(), hp: A.hp,
        virtualEquips: A.getVEquips(1).map(card => ({ name: card.name, cards: (card.cards || []).map(zone) })),
        physicalEquips: A.getCards('e').map(card => card.name), range: A.getEquipRange(),
        qinggang: A.hasSkill('qinggang_skill'), material: zone(weapon), gameOver: !!_status.over });
      const deathRevival = { before: deathState() };
      trial = 'death';
      await A.die().set('animate', false);
      deathRevival.dead = deathState();
      A.revive(2, false);
      deathRevival.revived = deathState();
      trial = null;
      return { columns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'target', 'state'],
        events, skillCalls, eligibility, trials, jiedao, deathRevival };
    } finally { proto.trigger = original; C.removeSkill(observer); }
  };
}

export function checkVirtualEquipCombat(result) {
  assert.deepEqual(result.trials.map(trial => trial.name), ['control', 'orphan', 'bagua-control', 'bagua-orphan']);
  const [control, orphan] = result.trials;
  assert.deepEqual([control.before.targetHp, control.before.attackerPhysical, control.before.attackerVirtual,
    control.before.attackerRange, control.before.qinggang, control.before.armor, control.before.armorSkill],
  [4, 0, false, 1, false, 'B:e', true]);
  assert.deepEqual([control.after.targetHp, control.after.sha, control.after.armor], [4, 'd', 'B:e']);
  assert.deepEqual([orphan.before.targetHp, orphan.before.weapon, orphan.before.attackerPhysical,
    orphan.before.attackerVirtual, orphan.before.attackerRange, orphan.before.qinggang, orphan.before.armor],
  [4, 'd', 0, true, 2, true, 'B:e']);
  assert.deepEqual([orphan.after.targetHp, orphan.after.sha, orphan.after.weapon, orphan.after.armor], [3, 'd', 'd', 'B:e']);
  assert.deepEqual(result.skillCalls.filter(row => ['control', 'orphan'].includes(row[0])).map(row => row.slice(0, 4)), [
    ['control', 'renwang_skill', 'begin', 'B'], ['control', 'renwang_skill', 'end', 'B'],
    ['orphan', 'qinggang_skill', 'begin', 'A'], ['orphan', 'qinggang_skill', 'end', 'A'],
  ]);
  assert.equal(result.skillCalls.find(row => row[0] === 'orphan' && row[2] === 'end')[4].armorSuppressed, true);
  const timings = name => result.events.filter(row => row[0] === name && row[1] === 'request').map(row => row[2]);
  assert.ok(timings('control').includes('shaCancelled') && !timings('control').includes('damage'));
  assert.ok(timings('orphan').includes('damage') && !timings('orphan').includes('shaCancelled'));
  const [baguaControl, baguaOrphan] = result.trials.slice(2);
  assert.deepEqual([baguaControl.before.armorName, baguaControl.before.targetShan, baguaControl.before.red,
    baguaControl.after.targetHp, baguaControl.after.sha, baguaControl.after.red, baguaControl.after.armor],
  ['bagua', 0, 'c', 4, 'd', 'd', 'B:e']);
  assert.deepEqual([baguaOrphan.before.armorName, baguaOrphan.before.targetShan, baguaOrphan.before.red,
    baguaOrphan.before.weapon, baguaOrphan.before.attackerPhysical, baguaOrphan.after.targetHp,
    baguaOrphan.after.sha, baguaOrphan.after.red, baguaOrphan.after.weapon, baguaOrphan.after.armor],
  ['bagua', 0, 'c', 'd', 0, 3, 'd', 'c', 'd', 'B:e']);
  assert.deepEqual(result.eligibility.map(row => [row.trial, row.timing, row.eligible]),
    [['bagua-control', 'chooseToUseBegin', true], ['bagua-orphan', 'chooseToUseBegin', false]]);
  assert.deepEqual(result.skillCalls.filter(row => row[0].startsWith('bagua')).map(row => row.slice(0, 4)), [
    ['bagua-control', 'bagua_skill', 'begin', 'B'], ['bagua-control', 'bagua_skill', 'end', 'B'],
    ['bagua-orphan', 'qinggang_skill', 'begin', 'A'], ['bagua-orphan', 'qinggang_skill', 'end', 'A'],
  ]);
  assert.ok(timings('bagua-control').includes('judge') && timings('bagua-control').includes('shaMiss'));
  assert.ok(!timings('bagua-orphan').includes('judge') && timings('bagua-orphan').includes('damage'));
  assert.deepEqual(result.jiedao, { targetHand: 0, targetPhysicalEquip: 0, virtualWeaponMaterials: ['d'],
    canTargetJiedao: true, canTargetGuohe: false, beforeWeapon: 'd', sourceHandBefore: 1,
    trick: 'd', afterWeapon: 'd', sourceHasWeapon: false, targetHandAfter: 0, sourceHandAfter: 0 });
  assert.deepEqual(result.skillCalls.filter(row => row[0] === 'jiedao').map(row => row.slice(0, 4)),
    [['jiedao', 'jiedao', 'begin', 'C'], ['jiedao', 'jiedao', 'end', 'C']]);
  for (const [name, alive, hp] of [['before', true, null], ['dead', false, 0], ['revived', true, 2]]) {
    const snapshot = result.deathRevival[name];
    assert.deepEqual([snapshot.alive, snapshot.out, snapshot.virtualEquips, snapshot.physicalEquips,
      snapshot.range, snapshot.qinggang, snapshot.material, snapshot.gameOver],
    [alive, false, [{ name: 'qinggang', cards: ['d'] }], [], 2, true, 'd', false]);
    if (hp !== null) assert.equal(snapshot.hp, hp);
  }
}
