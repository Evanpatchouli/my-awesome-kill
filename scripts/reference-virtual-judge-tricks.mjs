import assert from 'node:assert/strict';

export function installVirtualJudgeTricks() {
  window.__runVirtualJudgeTricks = async ({ lib, game, ui, get, _status, actor: A }) => {
    if (!_status.auto) ui.click.auto('forced');
    lib.config.game_speed = 'vvfast';
    const seats = [A];
    while (seats.length < 8) seats.push(seats.at(-1).nextSeat);
    const B = seats[1], C = seats[2], label = player => player ? 'ABCDEFGH'[seats.indexOf(player)] ?? null : null;
    if (B === game.zhu) throw new Error('Expected B to be a non-lord target');
    if (game.me === B || game.me === C) game.swapPlayer(A);
    const names = ['useCard', 'choosePlayerCard', 'chooseButton', 'discardPlayerCard', 'gainPlayerCard',
      'lose', 'discard', 'gain', 'die', 'cardsDiscard'];
    const timings = [...new Set(names.flatMap(name => ['', 'Before', 'Begin', 'End', 'After', 'Cancelled'].map(s => name + s)))];
    let trial = null, material = null, trick = null;
    const events = [], trials = [];
    const zone = card => { const pos = get.position(card, true), owner = get.owner(card); return owner && pos ? `${label(owner)}:${pos}` : pos ?? null; };
    const describe = card => ({ name: card.name, position: zone(card), isViewAsCard: !!card.isViewAsCard,
      isMaterial: card === material, materials: (card.cards || []).map(item => ({ name: item.name, position: zone(item), isMaterial: item === material })) });
    const state = () => ({ material: zone(material), known: [A, B, C].map(player => material.isKnownBy(player)),
      virtual: B.getVCards('j').map(describe), raw: B.getCards('j').map(describe),
      handB: B.getCards('h').includes(material), handC: C.getCards('h').includes(material), handA: A.getCards('h').includes(material),
      materialInDiscard: ui.discardPile.contains(material), aliveB: B.isAlive(), trick: trick ? zone(trick) : null });
    const record = (kind, timing, event) => {
      if (trial && timings.includes(timing)) events.push([trial, kind, timing, event.name, event.getParent()?.name ?? null,
        label(event.player), label(event.source), event.result?.links?.map(card => describe(card)) ?? null,
        !!event.finished, !!event._cancelled, state()]);
    };
    const observer = 'reference_virtual_judge_tricks_observer';
    lib.skill[observer] = { trigger: { global: timings }, firstDo: true, forced: true, silent: true, forceDie: true,
      filter: () => !!trial,
      async content(event, trigger) { record('callback', event.triggername, trigger); } };
    game.finishSkill(observer); A.addSkill(observer);
    game.globalEventHandlers.pushHandler('chooseButton', (event, { state: timing }) => {
      if (trial && timing === 'begin' && (event.getParent('guohe', true) || event.getParent('shunshou', true))) {
        event.ai = button => button.link?.name === 'lebu' && button.link?.isViewAsCard ? 100 : -100;
      }
    });
    for (const name of ['discardPlayerCard', 'gainPlayerCard']) {
      game.globalEventHandlers.pushHandler(name, (event, { state: timing }) => {
        if (trial && timing === 'begin') event.ai = button => button.link?.name === 'lebu' && button.link?.isViewAsCard ? 100 : -100;
      });
    }
    const proto = lib.element.GameEvent.prototype, original = proto.trigger;
    proto.trigger = function (timing) { record('request', timing, this); return original.apply(this, arguments); };
    try {
      for (const [action, holder] of [['guohe', 'B'], ['guohe', 'C'], ['shunshou', 'B'], ['shunshou', 'C'], ['death', 'B'], ['death', 'C']]) {
        trial = null; trick = null;
        if (!B.isAlive()) B.revive(3, false);
        for (const player of seats) {
          const held = player.getCards('hej'); if (held.length) await player.discard(held);
        }
        B.hp = B.maxHp = 4; B.update();
        material = game.createCard2('qinggang', 'diamond', 6);
        await B.addJudge({ name: 'lebu' }, [material]);
        await (holder === 'B' ? B : C).gain(material);
        if (action !== 'death') {
          trick = game.createCard2(action, 'club', 4);
          await A.gain(trick);
        }
        trial = `${action}:${holder}`;
        const before = state(), canUse = trick ? A.canUse(trick, B, false) : null;
        if (action === 'death') await B.die().set('animate', false);
        else await A.useCard(trick, B, false);
        trials.push({ name: trial, canUse, before, after: state() });
      }
      return { columns: ['trial', 'kind', 'timing', 'event', 'parent', 'player', 'source', 'selected',
        'finished', 'cancelled', 'state'], events, trials };
    } finally { proto.trigger = original; A.removeSkill(observer); }
  };
}

export function checkVirtualJudgeTricks(result) {
  assert.deepEqual(result.trials.map(item => item.name),
    ['guohe:B', 'guohe:C', 'shunshou:B', 'shunshou:C', 'death:B', 'death:C']);
  for (const item of result.trials) {
    const [action, holder] = item.name.split(':');
    assert.deepEqual([item.before.material, item.before.known, item.before.virtual.length,
      item.before.raw.length, item.before.raw[0].isViewAsCard, item.before.raw[0].position],
    [`${holder}:h`, [true, true, true], 1, 1, true, 'B:j']);
    assert.deepEqual([item.after.material, item.after.virtual.length, item.after.raw.length, item.after.aliveB],
      [action === 'shunshou' ? 'A:h' : 'd', 0, 0, action !== 'death']);
    if (action === 'death') {
      assert.deepEqual(item.after.known, holder === 'B' ? [false, false, false] : [true, true, true]);
    } else {
      assert.equal(item.canUse, true);
      assert.deepEqual([item.after.known, item.after.trick], [[true, true, true], 'd']);
      const resultTiming = action === 'guohe' ? 'discardPlayerCardEnd' : 'gainPlayerCardEnd';
      const selected = result.events.find(row => row[0] === item.name && row[1] === 'request' && row[2] === resultTiming)?.[7];
      assert.equal(selected?.length, 1);
      assert.deepEqual([selected[0].name, selected[0].isViewAsCard, selected[0].isMaterial], ['lebu', true, false]);
    }
    const lose = result.events.find(row => row[0] === item.name && row[1] === 'request' && row[2] === 'loseBefore' && row[5] === 'B');
    assert.equal(lose?.[4], action === 'shunshou' ? 'gain' : 'discard');
  }
}
