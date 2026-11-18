import assert from 'node:assert/strict';

export function installReferenceCompare() {
  window.__installReferenceCompare = async ({ lib, game, ui, get, _status }) => {
    const skills = (await import('/character/yijiang/skill.js')).default;
    for (const name of ['qiaoshui', 'qiaoshui2', 'qiaoshui3', 'jyzongshi', 'tianbian']) {
      if (!lib.skill[name]) {
        lib.skill[name] = skills[name];
        game.finishSkill(name);
      }
    }
    const samples = [['modified-win', 7, 9], ['tie', 7, 7], ['loss', 4, 10]];
    const trials = [];
    const events = [];
    let trial;
    const label = seat => Object.keys(trial.seats).find(key => trial.seats[key] === seat) || (seat ? 'other' : null);
    const id = card => Object.keys(trial.cards).find(key => trial.cards[key] === card) || (card ? 'other' : null);
    const bids = ['aBid', 'bBid'];
    const zone = card => {
      const owner = get.owner(card);
      const position = get.position(card, true);
      return owner ? `${label(owner)}:${position}` : position || null;
    };
    const bidZones = () => bids.map(key => zone(trial.cards[key]));
    const currentSkills = () => ['qiaoshui2', 'qiaoshui3'].filter(name => trial.seats.A.hasSkill(name));
    const points = event => [event?.num1 ?? null, event?.num2 ?? null];
    const capture = (kind, timing, event) => events.push([
      trial.row.sample, kind, timing, event.name, event.getParent()?.name || null, label(event.player),
      ...bidZones(), ...points(trial.compare), currentSkills().join(',') || null,
    ]);

    // Read both hands from each nominated seat while that private chooseCard
    // is active. The browser itself belongs to C throughout all three trials.
    function recordSelection(event, timing) {
      const { A, B, C } = trial.seats;
      const view = viewer => [A, B].map(owner => owner.getKnownCards(viewer).map(id));
      const publicCards = [...ui.arena.querySelectorAll('.card.thrownhighlight:not(.infohidden)')]
        .filter(node => node.getClientRects().length);
      trial.row.privateSelections.push({
        timing, chooser: label(event.player), offered: event.player.getCards(event.position).map(id),
        selected: (event.result?.cards || []).map(id), autochoose: !!event.result?.autochoose,
        handsKnownToA: view(A), handsKnownToB: view(B), handsKnownToC: view(C),
        publicCompareCount: trial.row.reveals.length,
        visibleCompareFaces: bids.filter(key => publicCards.some(node => node._cardid === trial.cards[key].cardid)),
      });
    }

    // These hooks select legal inputs through the real offline choice engine.
    // They leave the card/target filters, choice processing and result untouched.
    const configured = new WeakSet();
    function preferFixtureChoice(event, { state }) {
      if (!trial || state !== 'begin' || configured.has(event)) return;
      configured.add(event);
      const parent = event.getParent().name;
      if (event.name === 'chooseCard' && parent === 'chooseCardOL') {
        const desired = event.player === trial.seats.A ? trial.cards.aBid : trial.cards.bBid;
        event.ai = card => card === desired ? 10 : 0;
      } else if (event.name === 'chooseTarget' && ['qiaoshui_cost', 'qiaoshui3'].includes(parent)) {
        const desired = parent === 'qiaoshui_cost' ? trial.seats.B : trial.seats.D;
        event.ai = target => target === desired ? 10 : -10;
      }
    }
    for (const name of ['chooseCard', 'chooseTarget']) game.globalEventHandlers.pushHandler(name, preferFixtureChoice);
    for (const name of ['qiaoshui_cost', 'qiaoshui', 'jyzongshi', 'tianbian_number', 'qiaoshui3']) {
      game.globalEventHandlers.pushHandler(name, (event, { state }) => {
        if (trial) capture('content', state, event);
      });
    }

    const originalTrigger = lib.element.GameEvent.prototype.trigger;
    lib.element.GameEvent.prototype.trigger = function (timing) {
      if (trial) {
        const parent = this.getParent()?.name;
        if (timing === 'chooseToCompareBegin') {
          trial.compare = this;
          trial.row.fixedResultSupplied = !!this.fixedResult;
        }
        if (parent === 'chooseCardOL' && ['chooseCardBegin', 'chooseCardAfter'].includes(timing)) {
          recordSelection(this, timing);
          capture('request', timing, this);
        } else if (['chooseCardOLBegin', 'chooseCardOLAfter', 'chooseToCompareBegin', 'compareCardShowBefore',
                    'showCardsAfter', 'compare', 'compareFixing', 'chooseToCompareAfter',
                    'phaseUseEnd', 'phaseEnd', 'phaseAfter'].includes(timing)) {
          capture('request', timing, this);
        } else if (['loseAfter', 'gainAfter', 'cardsDiscardAfter'].includes(timing) &&
                   this.cards?.some(card => bids.includes(id(card)))) {
          capture('request', timing, this);
        }
        if (timing === 'chooseCardOLAfter') trial.row.collected = this.result.map(choice => choice.cards.map(id));
        if (['chooseTargetBegin', 'chooseTargetAfter'].includes(timing) && ['qiaoshui_cost', 'qiaoshui3'].includes(parent)) {
          capture('request', timing, this);
          if (timing === 'chooseTargetAfter') {
            trial.row.targetChoices.push({ skill: parent, accepted: this.result.bool, targets: (this.result.targets || []).map(label) });
          }
        }
        if (['useCard2', 'useCardAfter'].includes(timing) && this.player === trial.seats.A &&
            this.cards?.some(card => card === trial.cards.sha || card === trial.cards.trick)) {
          capture('request', timing, this);
          trial.row.cardTargets.push({ timing, card: id(this.cards[0]), targets: this.targets.map(label), skills: currentSkills() });
        }
        if (timing === 'chooseBoolAfter' && this.getParent().skill === 'jyzongshi') {
          trial.row.zongshiAccepted = this.result.bool;
        }
        if (timing === 'chooseToCompareAfter') {
          const result = this.result;
          trial.row.result = { bool: result.bool, tie: !!result.tie, winner: label(result.winner),
            cards: [id(result.player), id(result.target)], numbers: [result.num1, result.num2] };
        }
        if (timing === 'gainAfter' && parent === 'jyzongshi') trial.row.zongshiGain = this.cards.map(id);
      }
      return originalTrigger.apply(this, arguments);
    };
    const originalVideo = game.addVideo;
    game.addVideo = function (type, player, info) {
      if (trial && type === 'compare') {
        const [first, , second] = info;
        trial.row.reveals.push([first, second].map(face => [face[2], face[0], face[1]]));
        capture('display', '$compare', trial.compare);
      }
      return originalVideo.apply(this, arguments);
    };
    // The flip animation starts with hidden faces. Record actual removal of
    // that class on both public card copies in spectator C's rendered arena.
    const flips = new MutationObserver(records => {
      if (!trial) return;
      for (const change of records) {
        const node = change.target;
        if (!change.oldValue?.split(' ').includes('infohidden') || !node.matches('.card.thrownhighlight:not(.infohidden)')) continue;
        const key = bids.find(key => node._cardid === trial.cards[key].cardid);
        if (key && node.isConnected && node.getClientRects().length) trial.flipped.add(key);
      }
    });
    flips.observe(ui.arena, { subtree: true, attributes: true, attributeFilter: ['class'], attributeOldValue: true });

    function installDriver(name, timing, order, content) {
      lib.skill[name] = {
        trigger: { player: timing }, forced: true, silent: true, [order]: true,
        async content(event, trigger, player) {
          try {
            await content(event, trigger, player);
          } catch (error) {
            window.__referenceError = `${name}: ${error.stack || error}`;
          }
        },
      };
      game.finishSkill(name);
      game.addGlobalSkill(name);
    }
    async function drawHand(player, cards) {
      for (const card of cards.toReversed()) ui.cardPile.insertBefore(card, ui.cardPile.firstChild);
      await player.draw(cards.length);
    }
    installDriver('reference_compare_setup', 'phaseUseBefore', 'firstDo', async (event, trigger, A) => {
      if (A === game.me) {
        const hand = A.getCards('h');
        if (hand.length) await A.discard(hand);
        trigger.finish();
        return;
      }
      if (trial || trials.length === samples.length) return;
      const [sample, first, second] = samples[trials.length];
      for (const player of game.players) {
        const existing = player.getCards('he');
        if (existing.length) await player.discard(existing);
        player.maxHp = 4;
        player.hp = 4;
        player.hujia = 0;
        player.update();
      }
      const B = A.next === game.me ? A.previous : A.next;
      const D = game.players.find(seat => seat !== A && seat !== B && seat !== game.me && !A.inRange(seat));
      if (!D) throw new Error('Expected an AI seat outside ordinary sha range');
      const cards = {
        aBid: game.createCard2('tao', 'heart', first), aOther: game.createCard2('shan', 'diamond', 2),
        bBid: game.createCard2('shan', 'diamond', second), bOther: game.createCard2('tao', 'heart', 6),
        sha: game.createCard2('sha', 'spade', 7), trick: game.createCard2('juedou', 'club', 1),
      };
      await drawHand(A, [cards.aBid, cards.aOther]);
      await drawHand(B, [cards.bBid, cards.bOther]);
      A.addSkill(['qiaoshui', 'jyzongshi']);
      if (sample === 'modified-win') A.addSkill('tianbian_number');
      trial = {
        seats: { A, B, C: game.me, D }, cards, flipped: new Set(),
        row: { sample, turn: game.phaseNumber, skills: A.getSkills('invisible'),
          hands: [A, B].map(player => player.getCards('h').map(card => [card.name, get.suit(card), get.number(card)])),
          privateSelections: [], targetChoices: [], reveals: [], cardTargets: [] },
      };
      window.__referenceStages.push(`${sample}: hands ready`);
    });

    installDriver('reference_compare_play', 'phaseUseBegin', 'lastDo', async (event, trigger, A) => {
      if (!trial || trial.seats.A !== A) return;
      const { B, D } = trial.seats;
      const { sha, trick } = trial.cards;
      if (!trial.row.result) throw new Error('qiaoshui did not finish its compare');
      await drawHand(A, [sha, trick]);
      trial.row.nextCard = { skillsBefore: currentSkills(), canUseTrick: A.canUse(trick, B),
        canUseShaOnB: A.canUse(sha, B), canUseShaOnD: A.canUse(sha, D), targetEnabled2ForD: lib.filter.targetEnabled2(sha, A, D) };
      const use = A.useCard(sha, B);
      await use;
      Object.assign(trial.row.nextCard, { shaTargets: use.targets.map(label), skillsAfterSha: currentSkills(),
        hpAfterSha: [B.hp, D.hp] });
      if (A.canUse(trick, B)) {
        const second = A.useCard(trick, B);
        await second;
        trial.row.nextCard.secondCardTargets = second.targets.map(label);
      }
      window.__referenceStages.push(`${trial.row.sample}: next card settled`);
    });

    installDriver('reference_compare_finish', 'phaseAfter', 'lastDo', async (event, trigger, A) => {
      if (!trial || trial.seats.A !== A) return;
      capture('callback', 'phaseAfter', trigger);
      trial.row.final = { skills: currentSkills(), canUseTrick: A.canUse(trial.cards.trick, trial.seats.B),
        zones: bidZones(), spectatorSawFlip: bids.filter(key => trial.flipped.has(key)),
        gainedCardsKnownToC: A.getKnownCards(game.me).map(id).filter(key => bids.includes(key)) };
      trials.push(trial.row);
      A.removeSkill('qiaoshui');
      A.removeSkill('jyzongshi');
      if (trial.row.sample === 'modified-win') A.removeSkill('tianbian_number');
      trial = undefined;
      if (trials.length === samples.length) {
        flips.disconnect();
        lib.element.GameEvent.prototype.trigger = originalTrigger;
        game.addVideo = originalVideo;
        window.__referenceResult = { players: game.players.length, online: !!_status.connectMode, trials,
          eventColumns: ['sample', 'kind', 'timing', 'event', 'parent', 'player', 'aBid', 'bBid', 'num1', 'num2', 'tempSkills'], events };
        game.pause();
      }
    });
  };
}

export function checkReferenceCompare(result) {
  assert.equal(result.players, 8);
  assert.equal(result.online, false);
  assert.deepEqual(result.trials.map(trial => trial.sample), ['modified-win', 'tie', 'loss']);
  const rows = result.events.map(row => Object.fromEntries(result.eventColumns.map((key, index) => [key, row[index]])));
  for (const trial of result.trials) {
    const won = trial.sample === 'modified-win';
    const tie = trial.sample === 'tie';
    const own = ['aBid', 'aOther'];
    const theirs = ['bBid', 'bOther'];
    assert.equal(trial.fixedResultSupplied, false);
    assert.deepEqual(trial.privateSelections.map(row => [row.chooser, row.timing, row.selected]), [
      ['A', 'chooseCardBegin', []], ['A', 'chooseCardAfter', ['aBid']],
      ['B', 'chooseCardBegin', []], ['B', 'chooseCardAfter', ['bBid']],
    ]);
    for (const row of trial.privateSelections) {
      assert.deepEqual(row.handsKnownToA, [own, []]);
      assert.deepEqual(row.handsKnownToB, [[], theirs]);
      assert.deepEqual(row.handsKnownToC, [[], []]);
      assert.equal(row.publicCompareCount, 0);
      assert.deepEqual(row.visibleCompareFaces, []);
      assert.equal(row.autochoose, false);
      assert.deepEqual(row.offered, row.chooser === 'A' ? own : theirs);
    }
    assert.deepEqual(trial.collected, [['aBid'], ['bBid']]);
    assert.deepEqual(trial.reveals, [[trial.hands[0][0], trial.hands[1][0]]]);
    assert.deepEqual(trial.final.spectatorSawFlip, ['aBid', 'bBid']);
    assert.deepEqual(trial.result, { bool: won, tie, winner: won ? 'A' : tie ? null : 'B',
      cards: ['aBid', 'bBid'], numbers: won ? [13, 9] : tie ? [7, 7] : [4, 10] });
    assert.equal(trial.zongshiAccepted, true);
    assert.deepEqual(trial.zongshiGain, [won ? 'bBid' : 'aBid']);
    assert.deepEqual(trial.final.gainedCardsKnownToC, trial.zongshiGain);
    assert.deepEqual(trial.final.zones, won ? ['d', 'A:h'] : ['A:h', 'd']);
    const targetChoices = [{ skill: 'qiaoshui_cost', accepted: true, targets: ['B'] }];
    if (won) targetChoices.push({ skill: 'qiaoshui3', accepted: true, targets: ['D'] });
    assert.deepEqual(trial.targetChoices, targetChoices);
    const next = trial.nextCard;
    assert.equal(next.canUseShaOnB, true);
    assert.equal(next.canUseShaOnD, false);
    assert.equal(next.targetEnabled2ForD, true);
    assert.equal(next.canUseTrick, won);
    assert.deepEqual(next.skillsBefore, [won ? 'qiaoshui3' : 'qiaoshui2']);
    assert.deepEqual(next.skillsAfterSha, won ? [] : ['qiaoshui2']);
    assert.deepEqual(next.shaTargets, won ? ['B', 'D'] : ['B']);
    assert.deepEqual(next.hpAfterSha, won ? [3, 3] : [3, 4]);
    assert.deepEqual(next.secondCardTargets, won ? ['B'] : undefined);
    assert.deepEqual(trial.cardTargets[0], { timing: 'useCard2', card: 'sha', targets: ['B'], skills: next.skillsBefore });
    const sampleRows = rows.filter(row => row.sample === trial.sample);
    const moves = sampleRows.filter(row => row.timing === 'loseAfter' && row.parent === 'loseAsync');
    assert.deepEqual(moves.map(row => [row.player, row.aBid, row.bBid]), [['A', 'o', 'B:h'], ['B', 'o', 'o']]);
    const phases = sampleRows.filter(row => ['phaseEnd', 'phaseAfter'].includes(row.timing));
    assert.deepEqual(phases.map(row => [row.kind, row.timing, row.tempSkills]), [
      ['request', 'phaseEnd', won ? null : 'qiaoshui2'], ['request', 'phaseAfter', won ? null : 'qiaoshui2'], ['callback', 'phaseAfter', null],
    ]);
    if (won) {
      const change = sampleRows.filter(row => row.event === 'tianbian_number');
      assert.deepEqual(change.map(row => [row.timing, row.num1, row.num2]), [['begin', 7, 9], ['end', 13, 9]]);
      const extraChoice = sampleRows.find(row => row.timing === 'chooseTargetBegin' && row.parent === 'qiaoshui3');
      assert.equal(extraChoice.tempSkills, null);
    }
    assert.deepEqual(trial.final.skills, []);
    assert.equal(trial.final.canUseTrick, true);
  }
}
