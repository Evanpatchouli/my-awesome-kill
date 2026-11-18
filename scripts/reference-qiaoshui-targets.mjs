import assert from 'node:assert/strict';

export function installQiaoshuiTargets() {
  window.__installQiaoshuiTargets = async ({ lib, game, ui, get, _status }) => {
    const yijiang = (await import('/character/yijiang/skill.js')).default;
    const standard = (await import('/character/standard/skill.js')).default;
    for (const [name, definition] of Object.entries({ qiaoshui2: yijiang.qiaoshui2,
      qiaoshui3: yijiang.qiaoshui3, jizhi: standard.jizhi })) {
      if (!lib.skill[name]) {
        lib.skill[name] = definition;
        game.finishSkill(name);
      }
    }

    let seats;
    let cards;
    let sample;
    const trials = [];
    const events = [];
    const seat = player => Object.keys(seats).find(key => seats[key] === player) ?? (player ? 'other' : null);
    const id = card => Object.keys(cards).find(key => cards[key] === card) ?? null;
    const seatsOf = targets => (targets || []).map(seat);
    const zone = card => {
      const owner = get.owner(card);
      const position = get.position(card, true);
      return owner ? `${seat(owner)}:${position}` : position || null;
    };
    const hasBonus = player => !!player?.hasSkill('qiaoshui3');
    const rootTimings = new Set(['useCard1', 'useCard2', 'useCard', 'useCardAfter']);
    const toTimings = new Set(['useCardToPlayer', 'useCardToPlayered', 'useCardToTarget', 'useCardToTargeted']);
    const choiceTimings = new Set(['chooseTargetBegin', 'chooseTargetAfter', 'chooseControlBegin', 'chooseControlAfter',
      'chooseToUseBegin', 'chooseToUseAfter', 'chooseToRespondBegin', 'chooseToRespondAfter']);
    const moveTimings = new Set(['loseAfter', 'gainAfter', 'cardsDiscardAfter']);

    function capture(kind, timing, event) {
      const parent = event.getParent()?.name;
      const material = event.cards?.map(id).filter(Boolean) ?? [];
      if (kind === 'request') {
        const isChoice = choiceTimings.has(timing) && (['qiaoshui3', 'jiedaoContentBefore'].includes(parent) ||
          (event.respondTo && ['B', 'D', 'E'].includes(seat(event.player)) &&
            (parent === 'jiedao' || parent === 'sha' || parent === 'nanman' || event.type === 'wuxie')));
        const isUse = rootTimings.has(timing) && material.length;
        const isTarget = toTimings.has(timing) && event.card?.name === 'jiedao';
        const isMove = moveTimings.has(timing) && material.length;
        if (!isChoice && !isUse && !isTarget && !isMove) return;
      }
      const row = { sample: sample.name, kind, timing, event: event.name, parent, player: seat(event.player),
        bonus: hasBonus(event.player), mainZone: zone(sample.card) };
      if (event.name === 'chooseTarget') {
        row.forced = !!event.forced;
        if (event.preTarget) row.holder = seat(event.preTarget);
        if (timing === 'chooseTargetBegin') {
          row.legal = Object.values(seats).filter(target => event.filterTarget(null, event.player, target)).map(seat);
        }
      }
      if (event.name === 'chooseControl' && timing === 'chooseControlAfter') {
        row.controls = event.controls;
        row.choiceList = event.choiceList;
      }
      if (event.result && choiceTimings.has(timing)) {
        row.result = {};
        if (typeof event.result.bool === 'boolean') row.result.accepted = event.result.bool;
        if (event.result.targets?.length) row.result.targets = seatsOf(event.result.targets);
        if (event.result.cards?.length) row.result.cards = event.result.cards.map(id);
        if (event.result.control) row.result.control = event.result.control;
      }
      if (event.respondTo) row.respondTo = [seat(event.respondTo[0]), event.respondTo[1]?.name || null];
      if (event.sourcex) row.requiredTarget = seat(event.sourcex);
      if (event.target) row.target = seat(event.target);
      if (event.targets) row.targets = seatsOf(event.targets);
      if (event.addedTargets) row.pairedTargets = seatsOf(event.addedTargets);
      if (event.addedTarget) row.pairedTarget = seat(event.addedTarget);
      if (material.length) row.material = material.map(name => [name, zone(cards[name])]);
      events.push(row);
    }

    // Changes here only give the normal offline choice engine preferences. The
    // card filters, requests, results, event contents and mutations are upstream.
    for (const name of ['chooseTarget', 'chooseControl', 'chooseToUse', 'chooseToRespond']) {
      game.globalEventHandlers.pushHandler(name, (event, { state }) => {
        if (!sample || state !== 'begin') return;
        const parent = event.getParent().name;
        if (name === 'chooseTarget' && parent === 'qiaoshui3') {
          event.ai = target => target === sample.preference ? 10 : -10;
        } else if (name === 'chooseTarget' && parent === 'jiedaoContentBefore') {
          event.ai = target => target === seats.C ? 10 : -10;
        } else if (name === 'chooseControl' && parent === 'qiaoshui3') {
          event.ai = () => 'cancel2';
        } else if (name === 'chooseToUse' && parent === 'jiedao') {
          event.ai1 = () => event.player === seats.B ? 10 : -10;
          event.ai2 = target => target === event.sourcex ? 10 : -10;
        } else if (name === 'chooseToUse' && (parent === 'sha' || event.type === 'wuxie')) {
          event.ai1 = () => 10;
        } else if (name === 'chooseToRespond' && parent === 'nanman') {
          event.ai = () => 10;
        }
      });
    }
    for (const name of ['qiaoshui3', 'jizhi', 'jiedaoContentBefore', 'jiedao', 'nanman', 'tiesuo', 'respond']) {
      game.globalEventHandlers.pushHandler(name, (event, { state }) => {
        if (sample && (name !== 'nanman' || event.player === seats.A)) capture('content', state, event);
      });
    }
    const proto = lib.element.GameEvent.prototype;
    const originalTrigger = proto.trigger;
    proto.trigger = function (timing) {
      if (sample) capture('request', timing, this);
      return originalTrigger.apply(this, arguments);
    };

    function driver(name, timing, content) {
      lib.skill[name] = {
        trigger: { player: timing }, forced: true, silent: true, firstDo: true,
        async content(event, trigger, player) {
          try {
            await content(trigger, player);
          } catch (error) {
            window.__referenceError = `${name}: ${error.stack || error}`;
          }
        },
      };
      game.finishSkill(name);
      game.addGlobalSkill(name);
    }

    async function run(name, card, targets, preference) {
      const { A } = seats;
      A.addTempSkill('qiaoshui3');
      const useCount = A.getStat('card')[card.name] || 0;
      const row = { name, initial: { targets: seatsOf(targets), mainZone: zone(card), bonus: hasBonus(A) } };
      sample = { name, card, preference };
      const use = A.useCard(card, targets.slice());
      await use;
      row.final = { targets: seatsOf(use.targets), pairedTargets: seatsOf(use.addedTargets), mainZone: zone(card),
        bonus: hasBonus(A), useCount: A.getStat('card')[card.name] - useCount };
      trials.push(row);
      sample = undefined;
      window.__referenceStages.push(`${name}: resolved`);
      return row;
    }

    let expiry;
    driver('reference_qiaoshui_expiry', 'phaseAfter', async (trigger, player) => {
      if (player !== seats?.A) return;
      expiry.callback = { restricted: player.hasSkill('qiaoshui2'), canUse: player.canUse(cards.expiry, seats.B) };
      proto.trigger = originalTrigger;
      window.__referenceResult = { players: game.players.length, online: !!_status.connectMode, trials, expiry, events };
      game.pause();
    });

    driver('reference_qiaoshui_play', 'phaseUseBefore', async (trigger, A) => {
      if (seats) return;
      const B = A.next;
      const C = B.next;
      const D = C.next;
      const E = D.next;
      if ([A, B, C, D, E].includes(game.me)) {
        if (A === game.me && A.countCards('h')) await A.discard(A.getCards('h'));
        trigger.finish();
        return;
      }
      const ordered = [A];
      for (let player = A.next; player !== A; player = player.next) ordered.push(player);
      seats = Object.fromEntries(ordered.map((player, index) => ['ABCDEFGH'[index], player]));
      const spectator = seat(game.me);
      for (const player of ordered) {
        const held = player.getCards('he');
        if (held.length) await player.discard(held);
        player.hp = player.maxHp = 4;
        player.hujia = 0;
        player.update();
      }
      cards = {
        jiedao: game.createCard2('jiedao', 'club', 12),
        bWeapon: game.createCard2('qinglong', 'spade', 5), dWeapon: game.createCard2('zhuge', 'diamond', 1),
        borrowedSha: game.createCard2('sha', 'spade', 7), refusedSha: game.createCard2('sha', 'club', 8),
        shan: game.createCard2('shan', 'heart', 2), jizhiDraw: game.createCard2('tao', 'heart', 6),
        nanman: game.createCard2('nanman', 'spade', 13),
        tiesuo: game.createCard2('tiesuo', 'club', 12), wuxie: game.createCard2('wuxie', 'spade', 11),
        expiry: game.createCard2('juedou', 'club', 1),
      };
      await A.gain([cards.jiedao, cards.nanman, cards.tiesuo, cards.expiry]);
      await B.equip(cards.bWeapon);
      await D.equip(cards.dWeapon);
      await B.gain(cards.borrowedSha);
      await D.gain(cards.refusedSha);
      await E.gain(cards.shan);
      ui.cardPile.prepend(cards.jizhiDraw);
      A.addSkill('jizhi');
      E.addTempSkill('qiaoshui3');
      if (!A.canUse(cards.jiedao, D) || !get.info(cards.jiedao).filterAddedTarget(cards.jiedao, A, E, D)) {
        throw new Error('The initial borrowed-sword pair must be legal');
      }
      // Leave both resulting requests playable even if the pair indices drift.
      if (!B.canUse(cards.borrowedSha, E) || !D.canUse(cards.refusedSha, C)) {
        throw new Error('Both borrowers must be able to use their physical Sha');
      }
      const borrowed = await run('borrowed-swords', cards.jiedao, [D, E], B);
      borrowed.final.effects = { weapons: [zone(cards.bWeapon), zone(cards.dWeapon)],
        slash: [zone(cards.borrowedSha), zone(cards.refusedSha)], shan: zone(cards.shan),
        jizhiDraw: zone(cards.jizhiDraw), hp: [C.hp, E.hp], shanBonus: hasBonus(E),
        borrowedShaUseCount: B.getStat('card').sha || 0 };
      A.removeSkill('jizhi');

      D.addTempSkill('qiaoshui3');
      const all = ordered.filter(player => A.canUse(cards.nanman, player));
      const automatic = await run('automatic-remove', cards.nanman, all, game.me);
      automatic.final.effects = { removed: spectator, hp: ordered.map(player => player.hp),
        response: zone(cards.refusedSha), responderBonus: hasBonus(D) };

      await D.gain(cards.wuxie);
      const cancel = await run('both-cancel', cards.tiesuo, [B, C]);
      cancel.final.effects = { linked: ordered.filter(player => player.isLinked()).map(seat),
        wuxie: zone(cards.wuxie), nullifierBonus: hasBonus(D) };

      A.addTempSkill('qiaoshui2');
      expiry = { before: { restricted: A.hasSkill('qiaoshui2'), canUse: A.canUse(cards.expiry, B) } };
      proto.trigger = function (timing) {
        if (timing === 'phaseAfter' && this.player === A) {
          expiry.request = { restricted: A.hasSkill('qiaoshui2'), canUse: A.canUse(cards.expiry, B) };
        }
        return originalTrigger.apply(this, arguments);
      };
      trigger.finish();
    });
  };
}

export function checkQiaoshuiTargets(result) {
  assert.equal(result.players, 8);
  assert.equal(result.online, false);
  assert.deepEqual(result.trials.map(trial => trial.name), ['borrowed-swords', 'automatic-remove', 'both-cancel']);
  const [borrowed, automatic, cancelled] = result.trials;
  const eventsFor = trial => result.events.filter(row => row.sample === trial.name);
  for (const [trial, name] of [[borrowed, 'jiedao'], [automatic, 'nanman'], [cancelled, 'tiesuo']]) {
    assert.equal(trial.initial.bonus, true);
    assert.equal(trial.final.bonus, false);
    assert.equal(trial.initial.mainZone, 'A:h');
    assert.equal(trial.final.mainZone, 'd');
    assert.equal(trial.final.useCount, 1);
    const paid = eventsFor(trial).filter(row => row.timing === 'loseAfter' && row.player === 'A' && row.parent === 'useCard');
    assert.deepEqual(paid.map(row => row.material), [[[name, 'o']]]);
  }

  const borrowedEvents = eventsFor(borrowed);
  const call = (timing, event = 'useCard') => borrowedEvents.findIndex(row => row.timing === timing && row.event === event);
  assert.deepEqual(borrowedEvents[call('useCard2')].targets, ['D']);
  assert.deepEqual(borrowedEvents[call('useCard2')].pairedTargets, ['E']);
  assert.deepEqual(borrowedEvents[call('useCard')].targets, ['D', 'B']);
  assert.deepEqual(borrowedEvents[call('useCard')].pairedTargets, ['E']);
  assert.ok(call('useCard2') < call('begin', 'qiaoshui3'));
  assert.ok(call('end', 'qiaoshui3') < call('useCard'));
  assert.ok(call('useCard') < call('begin', 'jizhi'));
  assert.ok(call('end', 'jizhi') < call('useCardToPlayer', 'useCardToPlayer'));
  assert.ok(borrowedEvents.findLastIndex(row => row.timing === 'useCardToTargeted') < call('begin', 'jiedaoContentBefore'));
  assert.deepEqual(borrowedEvents.filter(row => row.timing === 'chooseTargetBegin').map(row => row.legal), [
    ['B'], ['A', 'C', 'D', 'E', 'G', 'H'],
  ]);
  const borrowedChoices = borrowedEvents.filter(row => row.timing === 'chooseTargetAfter');
  assert.deepEqual(borrowedChoices.map(row => [row.player, row.holder, row.forced, row.result.targets]), [
    ['A', undefined, false, ['B']], ['A', 'B', true, ['C']],
  ]);
  assert.deepEqual(borrowed.final.targets, ['B', 'D']);
  assert.deepEqual(borrowed.final.pairedTargets, ['E', 'C', 'E']);
  assert.deepEqual(borrowedEvents.filter(row => row.timing === 'begin' && row.event === 'jiedao').map(row => [row.target, row.pairedTarget]), [
    ['B', 'E'], ['D', 'C'],
  ]);
  const attacks = borrowedEvents.filter(row => row.timing === 'chooseToUseAfter' && row.parent === 'jiedao');
  assert.deepEqual(attacks.map(row => [row.player, row.requiredTarget, row.result.accepted]), [['B', 'E', true], ['D', 'C', false]]);
  assert.deepEqual(borrowed.final.effects, { weapons: ['B:e', 'A:h'], slash: ['d', 'D:h'], shan: 'd',
    jizhiDraw: 'A:h', hp: [4, 4], shanBonus: false, borrowedShaUseCount: 0 });
  const shan = borrowedEvents.filter(row => row.timing === 'useCard2' && row.player === 'E');
  assert.deepEqual(shan.map(row => [row.parent, row.bonus, row.targets, row.material]), [['chooseToUse', true, [], [['shan', 'o']]]]);

  const automaticEvents = eventsFor(automatic);
  const { removed, hp, response, responderBonus } = automatic.final.effects;
  assert.deepEqual(automatic.final.targets, automatic.initial.targets.filter(target => target !== removed));
  assert.equal(hp['ABCDEFGH'.indexOf(removed)], 4);
  assert.deepEqual(automaticEvents.filter(row => row.timing === 'chooseTargetAfter').map(row => [row.forced, row.result.targets]), [[false, [removed]]]);
  assert.equal(response, 'd');
  assert.equal(responderBonus, true);
  assert.ok(automaticEvents.some(row => row.event === 'respond' && row.timing === 'begin' && row.player === 'D'));
  assert.ok(!automaticEvents.some(row => row.timing === 'useCard2' && row.player === 'D'));

  const cancelEvents = eventsFor(cancelled);
  assert.deepEqual(cancelled.final.targets, cancelled.initial.targets);
  const control = cancelEvents.find(row => row.timing === 'chooseControlAfter');
  assert.deepEqual(control.controls, ['选项一', '选项二', 'cancel2']);
  assert.deepEqual(control.choiceList.map(label => label.replace(/【[^】]+】/g, '')),
    ['为铁索连环增加一个目标', '为铁索连环减少一个目标']);
  assert.deepEqual(control.result, { control: 'cancel2' });
  assert.equal(control.bonus, false);
  assert.ok(!cancelEvents.some(row => row.event === 'chooseTarget'));
  assert.deepEqual(cancelled.final.effects, { linked: ['C'], wuxie: 'd', nullifierBonus: false });
  assert.deepEqual(cancelEvents.filter(row => row.timing === 'useCard2' && row.player === 'D')
    .map(row => [row.parent, row.bonus, row.targets, row.material]), [['_wuxie', true, [], [['wuxie', 'o']]]]);
  const restriction = { restricted: true, canUse: false };
  assert.deepEqual(result.expiry, { before: restriction, request: restriction, callback: { restricted: false, canUse: true } });
}
