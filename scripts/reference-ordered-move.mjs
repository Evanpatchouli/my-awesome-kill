import assert from 'node:assert/strict';

// The driver arranges physical cards and chooses an offline AI strategy. The
// skills, choice contents, scheduler, and card movements are upstream code.
export function installOrderedMove() {
  window.__installOrderedMove = async ({ lib, game, ui, get, _status }) => {
    const standard = (await import('/character/standard/skill.js')).default;
    const refresh = (await import('/character/refresh/skill.js')).default;
    for (const [name, skill] of [['guanxing', standard.guanxing], ['guanxing_fail', standard.guanxing_fail], ['xunxun', refresh.xunxun]]) {
      if (!lib.skill[name]) {
        lib.skill[name] = skill;
        game.finishSkill(name);
      }
    }

    const samples = ['guanxing', 'xunxun', 'guanxing-cancel'];
    const trials = [];
    const events = [];
    let trial;
    let falseChoice;
    const label = card => trial?.cards.indexOf(card) >= 0 ? `c${trial.cards.indexOf(card)}` : null;
    const labels = cards => (cards || []).map(label);
    const groups = moved => moved?.map(labels) || null;
    const positions = () => trial.cards.map(card => get.position(card, true) || null);
    const pile = () => {
      const cards = [...ui.cardPile.childNodes];
      return { top: labels(cards.slice(0, 5)), bottom: labels(cards.slice(-3)) };
    };
    const visible = card => [...document.querySelectorAll('.card')].some(node => {
      const same = node === card || (card.cardid && (node.cardid === card.cardid || node._cardid === card.cardid));
      return same && !node.classList.contains('infohidden') && !!node.getClientRects().length &&
        getComputedStyle(node).visibility !== 'hidden';
    });
    const privacy = () => ({
      knownToObserver: trial.cards.map(card => card.isKnownBy(game.me)),
      knownToActor: trial.cards.map(card => card.isKnownBy(trial.actor)),
      visibleToObserver: trial.cards.map(visible),
      observerIsActor: game.me === trial.actor,
    });
    const watched = new Set(['guanxing', 'xunxun', 'phaseZhunbei', 'phaseDraw',
      'chooseToGuanxing', 'chooseToMove', 'cardsGotoOrdering', 'cardsGotoPile', 'cardsDiscard']);
    const originalTrigger = lib.element.GameEvent.prototype.trigger;
    lib.element.GameEvent.prototype.trigger = function (timing) {
      if (falseChoice && this._referenceOptional &&
          ['Before', 'Begin', 'End', 'After', 'Cancelled'].some(suffix => timing === this.name + suffix)) {
        falseChoice.events.push([timing, this.getParent()?.name || null, this.result?.bool ?? null]);
      }
      if (trial && watched.has(this.name) &&
          ['Before', 'Begin', 'Begin1', 'End', 'After', 'Cancelled'].some(suffix => timing === this.name + suffix)) {
        const moved = groups(this.result?.moved);
        events.push([trial.name, timing, this.name, this.getParent()?.name || null,
          this.result?.bool ?? null, moved, positions()]);
        if (this.name === 'chooseToMove' && timing === 'chooseToMoveBegin') {
          trial.row.choiceStart = { parent: this.getParent()?.name || null, forced: !!this.forced,
            mine: this.isMine(), list: this.list.map(item => [item[0], labels(item[1])]),
            privacy: privacy() };
        }
        if (this.name === 'chooseToMove' && timing === 'chooseToMoveAfter') {
          trial.row.choiceResult = { bool: this.result?.bool ?? null, moved };
          trial.row.choiceEndPrivacy = privacy();
        }
        if (this.name === 'chooseToGuanxing' && timing === 'chooseToGuanxingAfter') {
          trial.row.parentResult = { bool: this.result?.bool ?? null, moved, pile: pile(), privacy: privacy() };
        }
        if (this.name === 'xunxun' && timing === 'xunxunAfter') trial.row.skillPile = pile();
      }
      return originalTrigger.apply(this, arguments);
    };

    game.globalEventHandlers.pushHandler('chooseBool', (event, { state }) => {
      if (trial && state === 'begin' && event.player === trial.actor) event.ai = () => 10;
    });
    game.globalEventHandlers.pushHandler('chooseToMove', (event, { state }) => {
      if (!trial || state !== 'begin' || event.player !== trial.actor || trial.name === 'guanxing-cancel') return;
      // This is an AI preference, not a fixed event result. The actual result is
      // produced by chooseToMove's normal processAI path.
      event.processAI = list => {
        const cards = list[0][1].slice();
        if (trial.name === 'guanxing') return [[cards[2], cards[0]], [cards[4], cards[1], cards[3]]];
        const moved = [cards.slice(), []];
        const probes = [{ operation: 'confirm', count: 0, allowed: event.filterOk(moved) }];
        for (const card of [cards[3], cards[1], cards[2]]) {
          const allowed = event.filterMove({ link: card }, 1, moved);
          probes.push({ operation: 'move', card: label(card), count: moved[1].length, allowed });
          if (allowed) {
            moved[0].splice(moved[0].indexOf(card), 1);
            moved[1].push(card);
          }
          probes.push({ operation: 'confirm', count: moved[1].length, allowed: event.filterOk(moved) });
        }
        trial.row.probes = probes;
        return [[cards[2], cards[0]], moved[1]];
      };
    });

    function driver(name, timing, order, content) {
      lib.skill[name] = {
        trigger: { player: timing }, forced: true, silent: true, [order]: true,
        async content(event, trigger, player) {
          try { await content(event, trigger, player); }
          catch (error) { window.__referenceError = `${name}: ${error.stack || error}`; }
        },
      };
      game.finishSkill(name);
      game.addGlobalSkill(name);
    }
    lib.skill.reference_order_cancel = {
      trigger: { global: 'chooseToMoveBegin' }, firstDo: true, forced: true, silent: true,
      filter: event => trial?.name === 'guanxing-cancel' && event.player === trial.actor && event.getParent()?.name === 'chooseToGuanxing',
      async content(event, trigger) { trigger.cancel(); },
    };
    game.finishSkill('reference_order_cancel');
    game.addGlobalSkill('reference_order_cancel');

    game.globalEventHandlers.pushHandler('phaseLoop', (event, { state }) => {
      if (state !== 'begin') return;
      if (!_status.auto) ui.click.auto('forced');
      lib.config.game_speed = 'vvfast';
      game.players.forEach(player => { player.noPhaseDelay = true; });
    });
    driver('reference_order_setup', 'phaseBefore', 'firstDo', async (event, trigger, actor) => {
      if (trial || actor === game.me || trials.length >= samples.length) return;
      const name = samples[trials.length];
      const cards = [
        game.createCard2('sha', 'spade', 3), game.createCard2('shan', 'heart', 4),
        game.createCard2('tao', 'diamond', 5), game.createCard2('jiu', 'club', 6),
        game.createCard2('wuxie', 'spade', 7),
      ];
      ui.cardPile.prepend(...cards);
      const skill = name === 'xunxun' ? 'xunxun' : 'guanxing';
      trial = { name, actor, cards, skill, row: { name,
        submission: name === 'guanxing-cancel' ? 'event.cancel at chooseToMoveBegin' : 'offline processAI',
        cards: cards.map(card => [card.name, get.suit(card), get.number(card)]) } };
      trial.row.initialPile = pile();
      actor.addSkill(skill);
      window.__referenceStages.push(`${name}: arranged`);
    });
    driver('reference_order_finish', 'phaseUseBefore', 'firstDo', async (event, trigger, actor) => {
      if (trial && actor === trial.actor) {
        trial.row.afterDraw = { hand: labels(actor.getCards('h')).filter(Boolean), positions: positions(),
          pile: pile(), privacy: privacy() };
        trials.push(trial.row);
        actor.removeSkill(trial.skill);
        window.__referenceStages.push(`${trial.name}: draw complete`);
        trial = undefined;
        if (trials.length === samples.length) {
          const cards = [game.createCard2('sha', 'club', 8), game.createCard2('shan', 'heart', 9)];
          await actor.gain(cards);
          const choice = actor.chooseToMove('optional fixture');
          falseChoice = { submission: 'offline processAI returns null', forced: !!choice.forced,
            mine: choice.isMine(), events: [] };
          choice._referenceOptional = true;
          choice.set('list', [['hand', cards], ['other']]);
          choice.set('processAI', () => null);
          const result = await choice.forResult();
          Object.assign(falseChoice, { bool: result?.bool ?? null, cancelled: !!choice._cancelled,
            positions: cards.map(card => get.position(card, true)), inHand: cards.map(card => actor.getCards('h').includes(card)) });
          lib.element.GameEvent.prototype.trigger = originalTrigger;
          window.__referenceResult = { players: game.players.length, online: !!_status.connectMode, trials, falseChoice,
            eventColumns: ['sample', 'timing', 'event', 'parent', 'bool', 'moved', 'positions'], events };
          game.pause();
        }
      }
      trigger.finish();
    });
  };
}

export function checkOrderedMove(result) {
  assert.equal(result.players, 8);
  assert.equal(result.online, false);
  assert.deepEqual(result.trials.map(trial => trial.name), ['guanxing', 'xunxun', 'guanxing-cancel']);
  const [guanxing, xunxun, cancelled] = result.trials;
  const unseen = [false, false, false, false, false];
  for (const trial of result.trials) {
    assert.deepEqual(trial.initialPile.top, ['c0', 'c1', 'c2', 'c3', 'c4']);
    assert.equal(trial.choiceStart.mine, false);
    assert.equal(trial.choiceStart.privacy.observerIsActor, false);
    assert.deepEqual(trial.choiceStart.privacy.knownToObserver, unseen);
    assert.deepEqual(trial.choiceStart.privacy.visibleToObserver, unseen);
  }
  assert.equal(guanxing.submission, 'offline processAI');
  assert.deepEqual(guanxing.choiceStart.list, [['牌堆顶', ['c0', 'c1', 'c2', 'c3', 'c4']], ['牌堆底', []]]);
  assert.equal(guanxing.choiceStart.parent, 'chooseToGuanxing');
  assert.equal(guanxing.choiceStart.forced, false);
  assert.deepEqual(guanxing.choiceResult, { bool: true, moved: [['c2', 'c0'], ['c4', 'c1', 'c3']] });
  assert.deepEqual(guanxing.choiceEndPrivacy.knownToObserver, unseen);
  assert.deepEqual(guanxing.choiceEndPrivacy.visibleToObserver, unseen);
  assert.deepEqual(guanxing.parentResult.moved, [['c0', 'c2'], ['c4', 'c1', 'c3']]);
  assert.equal(guanxing.parentResult.bool, true);
  assert.deepEqual(guanxing.parentResult.pile, { top: ['c2', 'c0', null, null, null], bottom: ['c4', 'c1', 'c3'] });
  assert.deepEqual(guanxing.parentResult.privacy.knownToActor, [true, true, true, true, true]);
  assert.deepEqual(guanxing.parentResult.privacy.knownToObserver, unseen);
  assert.deepEqual(guanxing.parentResult.privacy.visibleToObserver, unseen);
  assert.deepEqual(guanxing.afterDraw.hand, ['c2', 'c0']);
  assert.deepEqual(guanxing.afterDraw.pile.bottom, ['c4', 'c1', 'c3']);
  assert.deepEqual(guanxing.afterDraw.privacy.knownToObserver, unseen);
  assert.deepEqual(guanxing.afterDraw.privacy.visibleToObserver, unseen);

  assert.equal(xunxun.submission, 'offline processAI');
  assert.equal(xunxun.choiceStart.parent, 'xunxun');
  assert.equal(xunxun.choiceStart.forced, true);
  assert.deepEqual(xunxun.choiceStart.list, [['牌堆顶', ['c0', 'c1', 'c2', 'c3']], ['牌堆底', []]]);
  assert.deepEqual(xunxun.probes, [
    { operation: 'confirm', count: 0, allowed: false },
    { operation: 'move', card: 'c3', count: 0, allowed: true },
    { operation: 'confirm', count: 1, allowed: false },
    { operation: 'move', card: 'c1', count: 1, allowed: true },
    { operation: 'confirm', count: 2, allowed: true },
    { operation: 'move', card: 'c2', count: 2, allowed: false },
    { operation: 'confirm', count: 2, allowed: true },
  ]);
  assert.deepEqual(xunxun.choiceResult, { bool: true, moved: [['c2', 'c0'], ['c3', 'c1']] });
  assert.deepEqual(xunxun.choiceEndPrivacy.knownToObserver, unseen);
  assert.deepEqual(xunxun.choiceEndPrivacy.visibleToObserver, unseen);
  assert.deepEqual(xunxun.skillPile, { top: ['c2', 'c0', 'c4', null, null], bottom: [null, 'c3', 'c1'] });
  assert.deepEqual(xunxun.afterDraw.hand, ['c2', 'c0']);
  assert.deepEqual(xunxun.afterDraw.pile.top.slice(0, 1), ['c4']);
  assert.deepEqual(xunxun.afterDraw.pile.bottom.slice(-2), ['c3', 'c1']);
  assert.deepEqual(xunxun.afterDraw.privacy.knownToObserver, unseen);
  assert.deepEqual(xunxun.afterDraw.privacy.visibleToObserver, unseen);

  assert.equal(cancelled.submission, 'event.cancel at chooseToMoveBegin');
  assert.equal(cancelled.choiceResult, undefined);
  assert.deepEqual(cancelled.parentResult.moved, [[], []]);
  assert.equal(cancelled.parentResult.bool, true);
  assert.deepEqual(cancelled.afterDraw.positions, ['d', 'd', 'd', 'd', 'd']);
  assert.deepEqual(cancelled.afterDraw.hand, []);
  assert.deepEqual(result.falseChoice, {
    submission: 'offline processAI returns null', forced: false, mine: false,
    events: [['chooseToMoveBefore', 'reference_order_finish', null],
      ['chooseToMoveBegin', 'reference_order_finish', null],
      ['chooseToMoveEnd', 'reference_order_finish', false],
      ['chooseToMoveAfter', 'reference_order_finish', false]],
    bool: false, cancelled: false, positions: ['h', 'h'], inHand: [true, true],
  });

  const rows = result.events.map(row => Object.fromEntries(result.eventColumns.map((name, index) => [name, row[index]])));
  const forSample = sample => rows.filter(row => row.sample === sample);
  const names = (sample, event) => forSample(sample).filter(row => row.event === event).map(row => row.timing);
  for (const [sample, parent] of [['guanxing', 'chooseToGuanxing'], ['xunxun', 'xunxun']]) {
    assert.deepEqual(names(sample, 'chooseToMove'), ['chooseToMoveBefore', 'chooseToMoveBegin', 'chooseToMoveEnd', 'chooseToMoveAfter']);
    assert.ok(forSample(sample).filter(row => row.event === 'chooseToMove').every(row => row.parent === parent));
  }
  assert.deepEqual(names('guanxing-cancel', 'chooseToMove'), ['chooseToMoveBefore', 'chooseToMoveBegin', 'chooseToMoveCancelled']);
  assert.deepEqual(names('guanxing-cancel', 'cardsGotoPile'), []);
  assert.ok(forSample('guanxing-cancel').filter(row => row.event === 'chooseToMove')
    .every(row => row.parent === 'chooseToGuanxing'));
  const order = (sample, sequence) => {
    const timings = forSample(sample).map(row => row.timing);
    let previous = -1;
    for (const timing of sequence) {
      const next = timings.indexOf(timing, previous + 1);
      assert.ok(next > previous, `${sample}: missing or out-of-order ${timing}`);
      previous = next;
    }
  };
  order('guanxing', ['phaseZhunbeiBegin', 'guanxingBegin', 'chooseToGuanxingBefore', 'chooseToGuanxingBegin',
    'cardsGotoOrderingAfter', 'chooseToMoveBefore', 'chooseToMoveBegin', 'chooseToMoveEnd', 'chooseToMoveAfter',
    'cardsGotoPileAfter', 'chooseToGuanxingEnd', 'chooseToGuanxingAfter', 'guanxingAfter', 'phaseDrawEnd']);
  order('xunxun', ['phaseDrawBegin1', 'xunxunBegin', 'cardsGotoOrderingAfter', 'chooseToMoveBegin',
    'chooseToMoveAfter', 'cardsGotoPileAfter', 'xunxunAfter', 'phaseDrawEnd']);
  order('guanxing-cancel', ['chooseToMoveBegin', 'chooseToMoveCancelled', 'chooseToGuanxingEnd',
    'chooseToGuanxingAfter', 'cardsDiscardBegin', 'cardsDiscardEnd', 'guanxingAfter', 'phaseDrawEnd']);
  const cancelledRows = forSample('guanxing-cancel');
  assert.deepEqual(cancelledRows.find(row => row.timing === 'chooseToGuanxingAfter').positions, ['o', 'o', 'o', 'o', 'o']);
  assert.deepEqual(cancelledRows.find(row => row.timing === 'cardsDiscardEnd').positions, ['d', 'd', 'd', 'd', 'd']);
}
