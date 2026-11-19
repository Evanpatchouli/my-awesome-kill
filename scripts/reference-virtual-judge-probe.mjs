import assert from 'node:assert/strict';

export function installVirtualJudgeProbe() {
  window.__runVirtualJudgeProbe = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1], C = seats[2], label = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    if (B === game.me) game.swapPlayer(A);
    const names = ['gain', 'disableJudge', 'discard', 'lose', 'phaseJudge', 'judge', 'equip', 'cardsDiscard'];
    const timings = [...new Set(names.flatMap(name => ['', 'Before', 'Begin', 'End', 'After', 'Cancelled'].map(suffix => name + suffix)))];
    let trial = null, material = null, reveal = null, cancelEquip = false, cancelInner = null, cancelledInner = false;
    const events = [], trials = [];
    const zone = card => { const pos = get.position(card, true), owner = get.owner(card); return owner && pos ? `${label(owner)}:${pos}` : pos ?? null; };
    const describe = card => ({ name: card.name, itemType: get.itemtype(card) ?? null,
      isViewAsCard: !!card.isViewAsCard, position: zone(card), isMaterial: card === material,
      materials: (card.cards || []).map(item => ({ name: item.name, position: zone(item), isMaterial: item === material })) });
    const state = () => ({ material: zone(material), visible: [A, B, C].map(player => material.isKnownBy(player)),
      judges: B.getVCards('j').map(describe), rawJudges: B.getCards('j').map(describe),
      canAddLebu: B.canAddJudge('lebu'), disabled: B.isDisabledJudge(),
      placeholder: [...B.node.judges.childNodes].filter(card => card.classList.contains('feichu')).length,
      revealed: reveal ? zone(reveal) : null, equipment: B.getCards('e').map(describe) });
    const record = (kind, timing, event) => {
      if (trial && timings.includes(timing)) events.push([trial, kind, timing, event.name, event.getParent()?.name ?? null,
        label(event.player), !!event.finished, !!event._cancelled, state()]);
    };
    const observer = 'reference_virtual_judge_probe_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true,
      filter: () => !!trial,
      async content(event, trigger) {
        record('callback', event.triggername, trigger);
        if (cancelEquip && event.triggername === 'equipBefore' && trigger.player === B) trigger.cancel();
        if (cancelInner && !cancelledInner && event.triggername === 'loseBefore' && trigger.player === B &&
          (cancelInner === 'phaseJudge' ? trigger.getParent()?.name === 'phaseJudge' : !!trigger.getParent('disableJudge', true))) {
          cancelledInner = true;
          trigger.cancel();
        }
      } };
    game.finishSkill(observer); A.addSkill(observer);
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return original.apply(this, arguments); };
    try {
      for (const [receiver, next] of [['B', 'phaseJudge'], ['B', 'disableJudge'], ['C', 'phaseJudge'], ['C', 'disableJudge']]) {
        trial = null; reveal = null;
        if (B.isDisabledJudge()) await B.enableJudge();
        for (const player of seats) {
          const held = player.getCards('hej'); if (held.length) await player.discard(held);
        }
        material = game.createCard2('qinggang', 'diamond', 6);
        await B.addJudge({ name: 'lebu' }, [material]);
        trial = `${receiver}:${next}`;
        const beforeGain = state();
        await (receiver === 'B' ? B : C).gain(material);
        const afterGain = state();
        if (next === 'phaseJudge') {
          reveal = game.createCard2('sha', 'heart', 9); ui.cardPile.prepend(reveal);
          await B.phaseJudge();
        } else await B.disableJudge();
        trials.push({ name: trial, beforeGain, afterGain, afterNext: state() });
      }
      for (const [receiver, form] of [['B', 'material'], ['B', 'wrapper'], ['C', 'material'], ['C', 'wrapper']]) {
        trial = null; reveal = null;
        if (B.isDisabledJudge()) await B.enableJudge();
        for (const player of seats) {
          const held = player.getCards('hej'); if (held.length) await player.discard(held);
        }
        material = game.createCard2('qinggang', 'diamond', 6);
        await B.addJudge({ name: 'lebu' }, [material]);
        await (receiver === 'B' ? B : C).gain(material);
        const wrapper = B.getCards('j')[0];
        trial = `discard:${receiver}:${form}`;
        const before = state();
        await B.discard(form === 'material' ? material : wrapper);
        trials.push({ name: trial, before, after: state() });
      }
      for (const [receiver, next] of [['B', 'phaseJudge'], ['B', 'disableJudge'], ['C', 'phaseJudge'], ['C', 'disableJudge']]) {
        trial = null; reveal = null;
        if (B.isDisabledJudge()) await B.enableJudge();
        for (const player of seats) {
          const held = player.getCards('hej'); if (held.length) await player.discard(held);
        }
        material = game.createCard2('qinggang', 'diamond', 6);
        await B.addJudge({ name: 'lebu' }, [material]);
        await (receiver === 'B' ? B : C).gain(material);
        trial = `cancel-inner:${receiver}:${next}`;
        cancelInner = next; cancelledInner = false;
        const before = state();
        if (next === 'phaseJudge') {
          reveal = game.createCard2('sha', 'heart', 9); ui.cardPile.prepend(reveal);
          await B.phaseJudge();
        } else await B.disableJudge();
        trials.push({ name: trial, before, after: state(), cancelledInner });
        cancelInner = null;
      }
      for (const cancelled of [true, false]) {
        trial = null; reveal = null;
        if (B.isDisabledJudge()) await B.enableJudge();
        for (const player of seats) {
          const held = player.getCards('hej'); if (held.length) await player.discard(held);
        }
        material = game.createCard2('qinggang', 'diamond', 6);
        await B.gain(material);
        trial = cancelled ? 'equip-cancel' : 'equip-success';
        cancelEquip = cancelled;
        const before = state();
        const equip = B.equip(material); await equip;
        trials.push({ name: trial, before, after: state(), finished: !!equip.finished, cancelled: !!equip._cancelled });
        cancelEquip = false;
      }
      return { columns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'finished', 'cancelled', 'state'], events, trials };
    } finally { proto.trigger = original; A.removeSkill(observer); }
  };
}

export function checkVirtualJudgeProbe(result) {
  assert.equal(result.trials.length, 14);
  const trial = name => result.trials.find(item => item.name === name);
  for (const receiver of ['B', 'C']) {
    for (const next of ['phaseJudge', 'disableJudge']) {
      const item = trial(`${receiver}:${next}`);
      assert.deepEqual([item.beforeGain.material, item.beforeGain.visible, item.beforeGain.judges.length,
        item.beforeGain.rawJudges.length, item.beforeGain.canAddLebu], ['j', [true, true, true], 1, 1, false]);
      assert.deepEqual([item.afterGain.material, item.afterGain.visible, item.afterGain.judges[0].name,
        item.afterGain.rawJudges[0].position, item.afterGain.rawJudges[0].isViewAsCard, item.afterGain.canAddLebu],
      [`${receiver}:h`, [true, true, true], 'lebu', 'B:j', true, false]);
      assert.deepEqual([item.afterNext.material, item.afterNext.judges, item.afterNext.rawJudges,
        item.afterNext.disabled, item.afterNext.placeholder], ['d', [], [], next === 'disableJudge', next === 'disableJudge' ? 1 : 0]);
      if (next === 'phaseJudge') assert.equal(item.afterNext.revealed, 'd');
      const rows = result.events.filter(row => row[0] === item.name && row[1] === 'request');
      const loseBegin = rows.find(row => row[2] === 'loseBegin' && row[4] === (next === 'phaseJudge' ? 'phaseJudge' : 'discard'));
      const loseEnd = rows.find(row => row[2] === 'loseEnd' && row[4] === (next === 'phaseJudge' ? 'phaseJudge' : 'discard'));
      assert.deepEqual([loseBegin[8].judges.length, loseEnd[8].judges.length], [1, 0]);
      const cancelled = trial(`cancel-inner:${receiver}:${next}`);
      assert.deepEqual([cancelled.cancelledInner, cancelled.after.material, cancelled.after.judges.length,
        cancelled.after.rawJudges.length, cancelled.after.disabled], [true, `${receiver}:h`, 1, 1, next === 'disableJudge']);
      const cancellationRows = result.events.filter(row => row[0] === cancelled.name && row[1] === 'request');
      assert.ok(cancellationRows.some(row => row[2] === 'loseCancelled'));
      if (next === 'phaseJudge') {
        assert.ok(cancellationRows.some(row => row[2] === 'judge'));
        assert.equal(cancelled.after.revealed, 'd');
      }
    }
  }
  for (const [receiver, form, position, wrappers] of [['B', 'material', 'd', 1], ['B', 'wrapper', 'd', 0],
    ['C', 'material', 'C:h', 1], ['C', 'wrapper', 'd', 0]]) {
    const item = trial(`discard:${receiver}:${form}`);
    assert.deepEqual([item.after.material, item.after.judges.length, item.after.rawJudges.length, item.after.canAddLebu],
      [position, wrappers, wrappers, !wrappers]);
  }
  const cancelledEquip = trial('equip-cancel'), successfulEquip = trial('equip-success');
  assert.deepEqual([cancelledEquip.before.visible, cancelledEquip.after.visible, cancelledEquip.after.material,
    cancelledEquip.cancelled], [[false, true, false], [false, true, false], 'B:h', true]);
  assert.deepEqual([successfulEquip.before.visible, successfulEquip.after.visible, successfulEquip.after.material,
    successfulEquip.cancelled], [[false, true, false], [true, true, true], 'B:e', false]);
}
