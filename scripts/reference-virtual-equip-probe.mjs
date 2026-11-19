import assert from 'node:assert/strict';

export function installVirtualEquipProbe() {
  window.__runVirtualEquipProbe = async ({ lib, game, ui, get, _status, actor: A, scenario }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1], seat = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    if (B === game.me) game.swapPlayer(A);
    const materialTrials = scenario.endsWith('discard-first') ? ['discardBefore', 'loseBefore'] : ['loseBefore', 'discardBefore'];
    const timings = ['discardBefore', 'loseBefore', 'gainAfter', 'equipBefore', 'equipBegin', 'equipEnd',
      'replaceEquipBefore', 'replaceEquipAfter', 'cardsDiscardBefore', 'cardsDiscardAfter', 'disableJudgeEnd'];
    let active = null, material = null, handled = false, recording = false;
    const rows = [], trials = [], canCalls = [];
    const zone = card => { const pos = get.position(card, true), owner = get.owner(card); return owner && pos ? `${seat(owner)}:${pos}` : pos ?? null; };
    const originalCanEquip = B.canEquip;
    const describe = card => {
      const symbol = card?.cardSymbol, virtual = symbol && card[symbol];
      return { name: card?.name ?? null, position: card ? zone(card) : null,
        itemType: card ? get.itemtype(card) ?? null : null, viewAsCard: !!card?.isViewAsCard,
        isMaterial: card === material, subtypes: card ? get.subtypes(card, false) : [],
        materials: (card?.cards || []).map(item => ({ name: item.name, position: zone(item), isMaterial: item === material })),
        binding: virtual ? { name: virtual.name, materials: (virtual.cards || []).map(item =>
          ({ name: item.name, position: zone(item), isMaterial: item === material })) } : null };
    };
    const state = () => ({ material: describe(material), isMin: B.isMin(),
      canEquip: [originalCanEquip.call(B, material, false), originalCanEquip.call(B, material, true)],
      enabled: B.countEnabledSlot(1), empty: B.countEmptySlot(1), equipable: B.countEquipableSlot(1),
      vEquips: B.getVEquips(1).map(describe), physicalEquips: B.getCards('e').map(describe),
      vEquipByName: B.getVEquip('qinggang') ? describe(B.getVEquip('qinggang')) : null,
      equipByName: B.getEquip('qinggang') ? describe(B.getEquip('qinggang')) : null,
      equipRange: B.getEquipRange(), physicalCount: B.countCards('e'),
      qinggangSkill: { hasSkill: B.hasSkill('qinggang_skill'), inEquipSkills: B.getSkills('e').includes('qinggang_skill'),
        inAllSkills: B.getSkills().includes('qinggang_skill') },
      vJudges: B.getVCards('j').map(describe), physicalJudges: B.getCards('j').map(describe) });
    function record(kind, timing, event) {
      if (!active || !recording || !timings.includes(timing)) return;
      rows.push([active, kind, timing, event.name, event.getParent()?.name ?? null, seat(event.player), state()]);
    }
    B.canEquip = function (card, replace) {
      const result = originalCanEquip.call(this, card, replace);
      if (active && recording) canCalls.push({ trial: active, event: _status.event?.name ?? null,
        parent: _status.event?.getParent()?.name ?? null, replace: !!replace, argument: describe(card), result, state: state() });
      return result;
    };
    const observer = 'reference_virtual_equip_probe_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true,
      filter: () => !!active && recording,
      async content(event, trigger) {
        record('callback', event.triggername, trigger);
        if (!handled && event.triggername === active && trigger.player === B && trigger.getParent('disableJudge', true)) {
          handled = true;
          trials.at(-1).points.push({ point: 'before-gain', state: state() });
          await B.gain(material);
          trials.at(-1).points.push({ point: 'after-gain', state: state() });
          await B.equip(material);
          trials.at(-1).points.push({ point: 'after-equip', state: state() });
        }
      } };
    game.finishSkill(observer); A.addSkill(observer);
    const proto = lib.element.GameEvent.prototype, originalTrigger = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return originalTrigger.apply(this, arguments); };
    try {
      for (const timing of materialTrials) {
        recording = false; active = null;
        if (B.isDisabledJudge()) await B.enableJudge();
        for (const player of seats) {
          const held = player.getCards('hej'); if (held.length) await player.discard(held);
        }
        material = game.createCard2('qinggang', 'diamond', 6);
        await B.addJudge({ name: 'lebu' }, [material]);
        active = timing; handled = false; recording = true;
        trials.push({ timing, points: [{ point: 'initial', state: state() }] });
        await B.disableJudge();
        trials.at(-1).points.push({ point: 'after-disable', state: state() });
        recording = false;
      }
      return { order: materialTrials, columns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'state'],
        events: rows, canCalls, trials };
    } finally { proto.trigger = originalTrigger; B.canEquip = originalCanEquip; A.removeSkill(observer); }
  };
}

export function checkVirtualEquipProbe(result) {
  assert.equal(result.trials.length, 2);
  assert.deepEqual(new Set(result.order), new Set(['discardBefore', 'loseBefore']));
  const point = (trial, name) => trial.points.find(point => point.point === name).state;
  const [first, second] = result.trials;
  const initial = point(first, 'initial'), equipped = point(first, 'after-equip'), orphan = point(first, 'after-disable');
  assert.deepEqual([initial.isMin, initial.canEquip, initial.enabled, initial.empty, initial.equipable,
    initial.vEquipByName, initial.equipByName, initial.physicalCount, initial.equipRange],
  [false, [true, true], 1, 1, 1, null, null, 0, 1]);
  assert.deepEqual([equipped.material.position, equipped.physicalCount, equipped.equipRange,
    equipped.equipByName?.name, equipped.vEquipByName?.name], ['B:e', 1, 2, 'qinggang', 'qinggang']);
  assert.deepEqual([orphan.material.position, orphan.canEquip, orphan.empty, orphan.equipable,
    orphan.physicalCount, orphan.equipByName, orphan.vEquipByName?.name, orphan.equipRange],
  ['d', [false, true], 0, 1, 0, null, 'qinggang', 2]);
  assert.deepEqual(orphan.qinggangSkill, { hasSkill: true, inEquipSkills: true, inAllSkills: true });
  const secondInitial = point(second, 'initial'), secondEquip = point(second, 'after-equip');
  assert.deepEqual([secondInitial.physicalCount, secondInitial.vEquips[0].materials[0].isMaterial,
    secondInitial.vEquips[0].materials[0].position, secondInitial.canEquip], [0, false, 'd', [false, true]]);
  assert.deepEqual([secondEquip.material.position, secondEquip.physicalCount, secondEquip.vEquips.length], ['d', 0, 1]);
  assert.deepEqual(result.canCalls.map(call => [call.trial, call.event, call.replace, call.result]),
    [[first.timing, 'equip', false, true], [second.timing, 'equip', false, false]]);
  for (const trial of result.trials) {
    const rows = result.events.filter(row => row[0] === trial.timing && row[1] === 'request');
    assert.ok(rows.some(row => row[2] === 'replaceEquipBefore'));
    assert.ok(rows.some(row => row[2] === 'replaceEquipAfter'));
    assert.equal(rows.some(row => row[2] === 'cardsDiscardBefore'), trial === second);
  }
}
