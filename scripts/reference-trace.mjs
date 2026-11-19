import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { installTriggerOrderScenario } from './reference-trigger-order-cancel.mjs';
import { checkReferenceRecastCallbacks, installReferenceRecast } from './reference-recast.mjs';
import { checkRecastBatch, installRecastBatch } from './reference-recast-batch.mjs';
import { checkRecastBatchBoundaries, installRecastBatchBoundaries } from './reference-recast-batch-boundaries.mjs';
import { checkBatchGiveDeath, installBatchGiveDeath } from './reference-batch-give-death.mjs';
import { checkReferenceLinkedDamage, installReferenceLinkedDamage } from './reference-linked-damage.mjs';
import { checkReferenceCompare, installReferenceCompare } from './reference-compare.mjs';
import { checkQiaoshuiTargets, installQiaoshuiTargets } from './reference-qiaoshui-targets.mjs';
import { checkCompareBoundaries, installCompareBoundaries } from './reference-compare-boundaries.mjs';
import { checkCompareMultiple, installCompareMultiple } from './reference-compare-multiple.mjs';
import { checkReferenceHp, installReferenceHp } from './reference-hp.mjs';
import { checkReferenceMaxHp, installReferenceMaxHp } from './reference-max-hp.mjs';
import { checkReferenceRevival, installReferenceRevival } from './reference-revival.mjs';
import { checkReferenceExpansion, installReferenceExpansion } from './reference-expansion.mjs';
import { checkZhongyiBoundaries, installZhongyiBoundaries } from './reference-zhongyi-boundaries.mjs';
import { checkZhongyiTransitDeath, installZhongyiTransitDeath } from './reference-zhongyi-transit-death.mjs';
import { checkPhaseScheduling, installPhaseScheduling } from './reference-phase-scheduling.mjs';
import { checkPhaseQueue, installPhaseQueue } from './reference-phase-queue.mjs';
import { checkJudgmentZones, installJudgmentZones } from './reference-judgment-zones.mjs';
import { checkCounterPayment, installCounterPayment } from './reference-counter-payment.mjs';
import { checkOrderedMove, installOrderedMove } from './reference-ordered-move.mjs';

// This is an optional oracle runner. It needs a running, separate upstream dev
// server and Playwright; neither is a runtime dependency of the game.
const require = createRequire(import.meta.url);
const playwrightPath = process.env.REFERENCE_PLAYWRIGHT_PATH || 'playwright';
const { chromium } = require(playwrightPath);
const baseUrl = process.env.REFERENCE_URL || 'http://127.0.0.1:8081';
const executablePath = process.env.REFERENCE_CHROMIUM_EXECUTABLE_PATH || undefined;
const commit = 'c73270948d72949aa80009536b9fd5400767d091';

// SHA-256 of LF-normalized, unmodified files from the pinned revision. These
// checks identify the sources involved in this scenario, not the entire tree.
const sourceHashes = {
  '/noname/library/element/gameEvent.ts': '9a777f0f88f46d706bb6539c8ea2ae09c87d90e5ffad43c289e1a8423a2be4b8',
  '/noname/library/element/content.ts': '09951b61fe8d9199ed1eb8b373faaf206d683e24b2a93a75ffc2259330891e6c',
  '/noname/library/element/player.js': '9ec265664ffa48750bc4117e3204bffe367679b6d260a23254fa14d2fc3163d4',
  '/noname/game/index.js': 'da122319d82a384c8ef4ad501870657533a52d24b753a4bd46680e2a4ecd9d5c',
  '/noname/library/skill.js': 'd21356bbe6a5ee8768f2295f9c4572c3411f69343dc1b574a639ab3bd20742d6',
  '/noname/library/index.js': '49bf9ca0017c8de8d061ed8aedca908c911cb07bf8830fd2840efce51bf2df09',
  '/card/standard.js': '9461c568e3734daf9e4eb1382aba69a8cb5e6a036a0ca4aed1048152277ac301',
  '/mode/identity.js': '08c63dab211db754f42e00d10f6c11a1f5ced86891572f67f0cab8185731014c',
};

const argv = process.argv.slice(2);
const descriptions = new Map([
  ['sha-no-shan', 'ordinary physical sha against an adjacent AI target with no shan'],
  ['trigger-order-cancel', 'damage trigger ordering, cost, and cancellation for physical sha'],
  ['tiesuo-recast', 'default and custom loss and gain when recasting a physical tiesuo'],
  ['recast-callbacks', 'executed loss and gain callbacks when recasting a physical tiesuo'],
  ['recast-batch', 'physical batch recast with equipment, intercepted and cancelled transfers, and clanmingdian selection'],
  ['recast-batch-boundaries', 'cancelled batch loss wrapper and a batch fully transferred before payment'],
  ['batch-give-death', 'batch give when its recipient dies while the cards are in transit'],
  ['linked-damage', 'tiesuo, faen, armor, and fire sha propagation across three turns'],
  ['compare', 'private pindian choices, public reveal, qiaoshui and jyzongshi across three turns'],
  ['qiaoshui-targets', 'paired, automatic and selectable target changes after winning qiaoshui'],
  ['compare-boundaries', 'hidden hand knowledge and cancellation of paid pindian cards'],
  ['compare-multiple', 'gushe and twdanlie with private multiplayer pindian, fixing and cancellations'],
  ['zhongyi-expansion', 'zhongyi expansion, limited use and cleanup at the next round'],
  ['zhongyi-boundaries', 'active zhongyi, expansion interruptions, holder death and stacked damage'],
  ['zhongyi-transit-death', 'holder death during zhongyi material transit through the public special pile'],
  ['phase-scheduling', 'dangxian play segments and fangquan extra turns in one natural seat cycle'],
  ['phase-queue', 'insertPhase queue ordering, a nested append, and a queued player who dies'],
  ['judgment-zones', 'delayed-card and judgment-card zones across ordinary and cancelled judgments'],
  ['counter-payment', 'physical wuxie moved or blocked around selection and payment for a delayed card'],
  ['ordered-move', 'natural guanxing and xunxun ordered choices, privacy, and cancellation'],
  ['damage-callback-cancel', 'damage callback cancellation after HP change for physical sha'],
  ['hp-cancel', 'cancelling changeHpBefore and changeHpEnd during a linked armored fire sha'],
  ['max-hp', 'max HP lifecycles, cancellation, death, and real cross-pack skills'],
  ['revival', 'ordinary and event revival after real deaths, including max HP and cancellation boundaries'],
]);
let scenario = 'sha-no-shan';
const scenarioIndex = argv.indexOf('--scenario');
if (scenarioIndex !== -1) {
  scenario = argv[scenarioIndex + 1];
  argv.splice(scenarioIndex, 2);
}
if (!descriptions.has(scenario)) throw new Error(`Unknown scenario: ${scenario}`);
if (argv.length && (argv.length !== 2 || !['--check', '--write'].includes(argv[0]))) {
  throw new Error('Usage: node scripts/reference-trace.mjs [--scenario name] [--check|--write path]');
}
const isRecast = scenario === 'tiesuo-recast' || scenario === 'recast-callbacks';
const isBatchRecast = scenario === 'recast-batch' || scenario === 'recast-batch-boundaries';
if (['zhongyi-expansion', 'zhongyi-boundaries', 'zhongyi-transit-death', 'qiaoshui-targets', 'judgment-zones', 'ordered-move'].includes(scenario)) {
  sourceHashes['/character/standard/skill.js'] = 'ee769399395b93f3d8293cd5b880b8e708864329bd0d79a0b4f0d8bd6f855a9e';
}
if (scenario === 'ordered-move') {
  sourceHashes['/character/refresh/skill.js'] = '655952b182a855e6c2886c9a217eb928357c16bc8b0c83fd1d7127651da0ba39';
  sourceHashes['/noname/library/element/card.js'] = '5d3a07fadafa1c5661ec197edb8c71f613b3db1129933fdf7d8aa311f5811f8d';
}
if (isRecast || isBatchRecast || ['batch-give-death', 'linked-damage', 'qiaoshui-targets'].includes(scenario)) sourceHashes['/card/extra.js'] = '8a73678a49b4fc413d6dc0d506733e594fae619d91dc8013cd441a6cd107c35d';
if (isBatchRecast || ['batch-give-death', 'recast-callbacks', 'compare', 'compare-boundaries', 'compare-multiple', 'zhongyi-boundaries', 'zhongyi-transit-death'].includes(scenario)) sourceHashes['/noname/library/element/card.js'] = '5d3a07fadafa1c5661ec197edb8c71f613b3db1129933fdf7d8aa311f5811f8d';
if (isBatchRecast || ['batch-give-death', 'zhongyi-boundaries', 'zhongyi-transit-death', 'phase-scheduling', 'phase-queue', 'compare-multiple', 'counter-payment', 'ordered-move'].includes(scenario)) {
  sourceHashes['/noname/get/index.js'] = '8515ed7242d2638720180507a778cb273d8f33743eac12c5237d2f50e805823d';
}
if (['batch-give-death', 'zhongyi-transit-death', 'phase-scheduling', 'phase-queue', 'compare-multiple'].includes(scenario)) {
  sourceHashes['/noname/library/element/GameEvent/compilers/ContentCompilerBase.ts'] = 'ea0174cdbc2ce664532335df11e120efa45b5c35253ecba5f28a73e0271c6cee';
  sourceHashes['/noname/library/element/GameEvent/compilers/ArrayCompiler.ts'] = '78d495e8871751bc38749775c1b402f810e234547ab4e6a12565160d4950853e';
}
if (scenario === 'zhongyi-boundaries') {
  sourceHashes['/noname/ai/basic.js'] = 'c32e43e690563d685929838635a40878baeb3333f52a2f585638fbe37655d83d';
  sourceHashes['/noname/game/check.js'] = '802b65210f6dc7eb76e22af82e8f95706b445f9c3b15c297750f6202ec287c9d';
  sourceHashes['/noname/ui/click/index.js'] = '6c85a0c7a1ecf7bc23534349fb82de18a1c2f436b882189dae5341fa5e6d69e1';
  sourceHashes['/noname/ui/create/index.js'] = '460073e1c3a230a0c78daa4ada32c468b0c616fa3d13c633cc8519a41d582b44';
  sourceHashes['/noname/library/element/dialog.js'] = '24893150df03829703e56c3156f63c784d536f42d1168d77674a167124521b03';
}
if (['linked-damage', 'compare', 'qiaoshui-targets', 'phase-scheduling', 'max-hp'].includes(scenario)) sourceHashes['/character/yijiang/skill.js'] = '5b0eb965a8428535bf3cd7d369c261b8ad95cccbeac8d8c817cad3490643a54f';
if (['max-hp', 'revival'].includes(scenario)) sourceHashes['/character/sp/skill.js'] = 'a63785adcf4c9518b58ca05f6ea85559f8eb6ea3d89ebcd782498f445cde85f9';
if (scenario === 'phase-scheduling') {
  sourceHashes['/character/shenhua/skill.js'] = '7e87ec26c3f183b83878f886e3872bd78a0f69a5e51e7f46a64bfcc79362cd62';
}
if (scenario === 'compare-multiple') {
  sourceHashes['/character/sp/skill.js'] = 'a63785adcf4c9518b58ca05f6ea85559f8eb6ea3d89ebcd782498f445cde85f9';
  sourceHashes['/character/tw/skill.js'] = '0a3c6e0d103575cb61229fc484a930a0e61e635d928637b84bbe8b3bbc70d8b3';
}
if (scenario === 'recast-batch') {
  sourceHashes['/character/clan/skill.js'] = 'a3185d7b948af3a19ebe3681d9d1be28db077e5fb38bd3b1af32a3e2f1ffbc9e';
}

const browser = await chromium.launch({ headless: true, executablePath });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const errors = [];
let result;
try {
  await context.addInitScript(() => {
    // Reproducible character selection and shuffle; all tested operations are
    // still the original engine's operations.
    let seed = 0x21436587;
    Math.random = () => {
      seed = (seed + 0x6d2b79f5) >>> 0;
      let value = seed;
      value = Math.imul(value ^ (value >>> 15), value | 1);
      value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
      return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
    };
    localStorage.setItem('gplv3_noname_alerted', 'true');
    localStorage.setItem('noname_0.9_directstart', 'true');
    localStorage.setItem('noname_0.9_disable_extension', 'true');
  });

  if (scenario === 'trigger-order-cancel') {
    await context.addInitScript({ content: `window.__installTriggerOrderScenario = ${installTriggerOrderScenario.toString()};` });
  } else if (isRecast) {
    await context.addInitScript(installReferenceRecast);
  } else if (scenario === 'recast-batch') {
    await context.addInitScript(installRecastBatch);
  } else if (scenario === 'recast-batch-boundaries') {
    await context.addInitScript(installRecastBatchBoundaries);
  } else if (scenario === 'batch-give-death') {
    await context.addInitScript(installBatchGiveDeath);
  } else if (scenario === 'linked-damage') {
    await context.addInitScript(installReferenceLinkedDamage);
  } else if (scenario === 'hp-cancel') {
    await context.addInitScript(installReferenceHp);
  } else if (scenario === 'max-hp') {
    await context.addInitScript(installReferenceMaxHp);
  } else if (scenario === 'revival') {
    await context.addInitScript(installReferenceRevival);
  } else if (scenario === 'zhongyi-expansion') {
    await context.addInitScript(installReferenceExpansion);
  } else if (scenario === 'zhongyi-boundaries') {
    await context.addInitScript(installZhongyiBoundaries);
  } else if (scenario === 'zhongyi-transit-death') {
    await context.addInitScript(installZhongyiTransitDeath);
  } else if (scenario === 'phase-scheduling') {
    await context.addInitScript(installPhaseScheduling);
  } else if (scenario === 'ordered-move') {
    await context.addInitScript(installOrderedMove);
  } else if (scenario === 'phase-queue') {
    await context.addInitScript(installPhaseQueue);
  } else if (scenario === 'judgment-zones') {
    await context.addInitScript(installJudgmentZones);
  } else if (scenario === 'counter-payment') {
    await context.addInitScript(installCounterPayment);
  } else if (scenario === 'compare-boundaries') {
    await context.addInitScript(installCompareBoundaries);
  } else if (scenario === 'compare-multiple') {
    await context.addInitScript(installCompareMultiple);
  } else if (scenario === 'compare') {
    await context.addInitScript(installReferenceCompare);
  } else if (scenario === 'qiaoshui-targets') {
    await context.addInitScript(installQiaoshuiTargets);
  }

  const page = await context.newPage();
  page.on('pageerror', error => errors.push(error.stack || error.message));
  page.on('dialog', dialog => {
    errors.push(`Unexpected browser dialog: ${dialog.message()}`);
    void dialog.dismiss();
  });
  const response = await page.goto(baseUrl, { waitUntil: 'domcontentloaded', timeout: 30000 });
  assert.equal(response?.status(), 200, 'Upstream Vite entry must return HTTP 200');

  const actualHashes = await page.evaluate(async paths => {
    const hashes = {};
    for (const path of paths) {
      const { default: source } = await import(`${path}?raw`);
      const normalized = source.replace(/\r\n/g, '\n');
      const bytes = new TextEncoder().encode(normalized);
      const digest = await crypto.subtle.digest('SHA-256', bytes);
      hashes[path] = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
    }
    return hashes;
  }, Object.keys(sourceHashes));
  assert.deepEqual(actualHashes, sourceHashes, 'The server must expose the pinned scenario sources');

  await page.getByText('跳过向导', { exact: true }).first().click({ timeout: 45000 });
  await page.locator('.button.character.selectable').first().waitFor({ state: 'visible', timeout: 45000 });

  await page.evaluate(async scenario => {
    const { lib, game, ui, get, _status } = await import('/noname.js');
    window.__referenceStages = ['fixture installed'];

    // The fixture removes random characters' skills before gameStart skills
    // fire. It does not replace the rules being observed.
    lib.skill.reference_fixture_setup = {
      trigger: { global: 'gameStart' }, firstDo: true, forced: true, silent: true,
      filter: () => !window.__referenceSetupDone,
      async content() {
        try {
          window.__referenceSetupDone = true;
          for (const player of game.players) {
            for (const skill of player.getSkills('invisible').slice()) player.removeSkill(skill, true);
          }
          const remaining = game.players.flatMap(player => player.getSkills('invisible'));
          if (remaining.length) throw new Error(`Skills remained after setup: ${remaining.join(', ')}`);
          window.__referenceStages.push('gameStart: character skills removed');
        } catch (error) {
          window.__referenceError = `setup: ${error.stack || error}`;
        }
      },
    };

    // Run once inside the real first phaseUseBegin. The temporary skill only
    // prepares inputs and invokes public upstream Player/GameEvent methods.
    lib.skill.reference_fixture_use = {
      trigger: { global: 'phaseUseBegin' }, firstDo: true, forced: true, silent: true,
      filter: (event, player) => !window.__referenceUseDone && event.player === player,
      async content(event, trigger, actor) {
        try {
          window.__referenceUseDone = true;
          window.__referenceStages.push('phaseUseBegin: arranging scenario');
          const target = actor.next === game.me ? actor.previous : actor.next;
          if (!target || target === game.me || target === actor) throw new Error('No adjacent AI target');
          if (scenario === 'hp-cancel') {
            window.__referenceResult = await window.__runReferenceHp({ lib, game, ui, actor, target, human: game.me });
            game.pause();
            return;
          }
          if (scenario === 'max-hp') {
            window.__referenceResult = await window.__runReferenceMaxHp({ lib, game, ui, get, _status, actor });
            game.pause();
            return;
          }
          if (scenario === 'revival') {
            window.__referenceResult = await window.__runReferenceRevival({ lib, game, ui, get, _status, actor });
            game.pause();
            return;
          }
          const multiTurn = {
            'linked-damage': window.__runReferenceLinkedDamage,
            'zhongyi-expansion': window.__runReferenceExpansion,
            'zhongyi-boundaries': window.__runZhongyiBoundaries,
            'zhongyi-transit-death': window.__runZhongyiTransitDeath,
            'compare-boundaries': window.__runCompareBoundaries,
            'compare-multiple': window.__runCompareMultiple,
            'recast-batch': window.__runRecastBatch,
            'recast-batch-boundaries': window.__runRecastBatchBoundaries,
            'batch-give-death': window.__runBatchGiveDeath,
            'judgment-zones': window.__runJudgmentZones,
            'counter-payment': window.__runCounterPayment,
          };
          const run = multiTurn[scenario];
          if (run) {
            const result = await run({ lib, game, ui, get, _status, actor, target });
            if (result) {
              window.__referenceResult = result;
              game.pause();
            } else {
              trigger.finish();
              window.__referenceUseDone = false;
            }
            return;
          }
          if (scenario === 'tiesuo-recast' || scenario === 'recast-callbacks') {
            window.__referenceResult = await window.__runReferenceRecast({
              lib, game, ui, get, actor, target, observeCallbacks: scenario === 'recast-callbacks',
            });
            game.pause();
            return;
          }
          for (const player of [actor, target]) {
            const cards = player.getCards('he');
            if (cards.length) await player.discard(cards);
            player.maxHp = 4;
            player.hp = 4;
            player.hujia = 0;
            player.update();
          }
          lib.config.skip_shan = false;
          const card = game.createCard2('sha', 'spade', 7);
          await actor.gain(card, 'gain2');

          const setup = {
            players: game.players.length,
            actorHp: actor.hp,
            targetHp: target.hp,
            targetHujia: target.hujia,
            actorHand: actor.getCards('h').map(item => [item.name, get.suit(item), get.number(item)]),
            targetHand: target.getCards('h').map(item => item.name),
            actorEquips: actor.getCards('e').map(item => item.name),
            targetEquips: target.getCards('e').map(item => item.name),
            actorSkills: actor.getSkills('invisible'),
            targetSkills: target.getSkills('invisible'),
            turnOwnerIsActor: _status.currentPhase === actor,
            targetIsHuman: target.isMine(),
            cardIsPhysical: get.itemtype(card) === 'card',
            priorShaUses: actor.getStat('card').sha || 0,
            inRange: actor.inRange(target),
            canUse: actor.canUse(card, target),
            cardPosition: get.position(card, true),
          };
          if (!setup.inRange || !setup.canUse || !setup.turnOwnerIsActor || setup.targetIsHuman ||
              !setup.cardIsPhysical || setup.priorShaUses || setup.actorEquips.length || setup.targetEquips.length ||
              setup.actorSkills.length || setup.targetSkills.length || setup.targetHand.length || setup.actorHand.length !== 1) {
            throw new Error(`Invalid scenario setup: ${JSON.stringify(setup)}`);
          }

          const scenarioFixture = scenario === 'trigger-order-cancel'
            ? await window.__installTriggerOrderScenario({ actor, target, game, lib }) : null;
          const labels = player => player === actor ? 'A' : player === target ? 'B' : player ? 'other' : null;
          const events = [];
          const proto = lib.element.GameEvent.prototype;
          const skillCallbacks = [];
          if (scenario === 'damage-callback-cancel') {
            for (const timing of ['damage', 'damageCancelled', 'damageSource', 'damageEnd', 'damageAfter', 'shaDamage', 'useCardAfter']) {
              const skillId = 'reference_damage_hp_' + timing;
              lib.skill[skillId] = {
                trigger: { global: timing }, forced: true, silent: true, firstDo: true,
                async content(event, trigger) {
                  skillCallbacks.push({ timing, hp: target.hp, trigger: trigger.name, triggerParent: trigger.getParent()?.name, callbackParent: event.getParent()?.name });
                  if (timing === 'damage') trigger.cancel();
                },
              };
              game.finishSkill(skillId);
              actor.addSkill(skillId);
            }
          }
          const originalTrigger = proto.trigger;
          proto.trigger = function (name) {
            const row = [
              name, this.name, labels(this.player), labels(this.source), labels(this.target),
              target.hp, get.position(card, true),
            ];
            if (scenario === 'trigger-order-cancel') row.push(this.getParent()?.name || null);
            events.push(row);
            return originalTrigger.apply(this, arguments);
          };
          try {
            await actor.useCard(card, target);
          } finally {
            proto.trigger = originalTrigger;
          }
          if (scenario === 'trigger-order-cancel') {
            const { material, skillSequence, filterResults } = scenarioFixture;
            const choiceEvent = scenarioFixture.getChoiceEvent();
            const choice = choiceEvent && { player: labels(choiceEvent.player), options: choiceEvent.controls.slice(), result: { control: choiceEvent.result.control, index: choiceEvent.result.index }, parent: choiceEvent.getParent()?.name || null };
            const final = { targetHp: target.hp, cardPosition: get.position(card, true), inDiscard: ui.discardPile.contains(card), materialPosition: get.position(material, true), materialInDiscard: ui.discardPile.contains(material), useCount: actor.getStat('card').sha };
            const hooks = events.map(row => row[0]);
            if (!choice || choice.result.index !== 1 || !filterResults.includes(true) || !filterResults.includes(false) || skillSequence.map(item => item.skill).join(',') !== 'first,beta,alpha,cost,target,last' ||
                !hooks.includes('damageCancelled') || hooks.includes('damageEnd') || hooks.includes('damageAfter') ||
                !hooks.includes('useCardAfter') || !events.some(row => row[0] === 'cardsDiscardEnd' && row[7] === 'orderingDiscard') ||
                final.targetHp !== 4 || final.useCount !== 1 || !final.inDiscard || !final.materialInDiscard) {
              throw new Error(`Invalid cancellation trace: ${JSON.stringify({ choice, skillSequence, final })}`);
            }
            window.__referenceResult = { setup, skillSequence, filterResults, choice, eventColumns: ['hook', 'event', 'player', 'source', 'target', 'targetHp', 'cardPosition', 'parent'], events, final };
            game.pause();
            return;
          }
          window.__referenceResult = {
            ...(scenario === 'damage-callback-cancel' ? { skillCallbacks } : {}),
            setup,
            eventColumns: ['hook', 'event', 'player', 'source', 'target', 'targetHp', 'cardPosition'],
            events,
            final: {
              targetHp: target.hp,
              cardPosition: get.position(card, true),
              inActorHand: actor.getCards('h').includes(card),
              inDiscard: ui.discardPile.contains(card),
              useCount: actor.getStat('card').sha,
              targetAlive: target.isAlive(),
            },
          };
          game.pause();
        } catch (error) {
          window.__referenceError = `use: ${error.stack || error}`;
        }
      },
    };
    const fixtures = ['reference_fixture_setup'];
    if (scenario === 'compare') await window.__installReferenceCompare({ lib, game, ui, get, _status });
    else if (scenario === 'qiaoshui-targets') await window.__installQiaoshuiTargets({ lib, game, ui, get, _status });
    else if (scenario === 'phase-scheduling') await window.__installPhaseScheduling({ lib, game, ui, get, _status });
    else if (scenario === 'ordered-move') await window.__installOrderedMove({ lib, game, ui, get, _status });
    else if (scenario === 'phase-queue') window.__installPhaseQueue({ lib, game, ui, _status });
    else fixtures.push('reference_fixture_use');
    for (const skill of fixtures) {
      game.finishSkill(skill);
      game.addGlobalSkill(skill);
    }
  }, scenario);

  await page.locator('.button.character.selectable').first().click({ timeout: 45000 });
  if (scenario === 'trigger-order-cancel') {
    void (async () => {
      try {
        await page.locator('.control').getByText('Beta', { exact: true }).first().click({ timeout: 85000 });
      } catch { /* AI choice or scenario completion requires no click. */ }
    })();
  }
  try {
    await page.waitForFunction(() => window.__referenceResult || window.__referenceError, null, { timeout: 90000 });
  } catch (error) {
    const state = await page.evaluate(async () => {
      const { _status } = await import('/noname.js');
      return { stages: window.__referenceStages, event: _status.event?.name, stack: _status.eventManager?.eventStack?.map(item => item.name) };
    });
    throw new Error(`Timed out waiting for the scenario: ${JSON.stringify(state)}`, { cause: error });
  }
  const captured = await page.evaluate(() => ({ value: window.__referenceResult, error: window.__referenceError }));
  if (captured.error) throw new Error(captured.error);
  assert.deepEqual(errors, [], 'Upstream page errors occurred');
  if (scenario === 'linked-damage') checkReferenceLinkedDamage(captured.value);
  if (scenario === 'hp-cancel') checkReferenceHp(captured.value);
  if (scenario === 'max-hp') checkReferenceMaxHp(captured.value);
  if (scenario === 'revival') checkReferenceRevival(captured.value);
  if (scenario === 'compare') checkReferenceCompare(captured.value);
  if (scenario === 'qiaoshui-targets') checkQiaoshuiTargets(captured.value);
  if (scenario === 'compare-boundaries') checkCompareBoundaries(captured.value);
  if (scenario === 'compare-multiple') checkCompareMultiple(captured.value);
  if (scenario === 'zhongyi-expansion') checkReferenceExpansion(captured.value);
  if (scenario === 'zhongyi-boundaries') checkZhongyiBoundaries(captured.value);
  if (scenario === 'zhongyi-transit-death') checkZhongyiTransitDeath(captured.value);
  if (scenario === 'phase-scheduling') checkPhaseScheduling(captured.value);
  if (scenario === 'ordered-move') checkOrderedMove(captured.value);
  if (scenario === 'phase-queue') checkPhaseQueue(captured.value);
  if (scenario === 'judgment-zones') checkJudgmentZones(captured.value);
  if (scenario === 'counter-payment') checkCounterPayment(captured.value);
  if (scenario === 'recast-callbacks') checkReferenceRecastCallbacks(captured.value);
  if (scenario === 'recast-batch') checkRecastBatch(captured.value);
  if (scenario === 'recast-batch-boundaries') checkRecastBatchBoundaries(captured.value);
  if (scenario === 'batch-give-death') checkBatchGiveDeath(captured.value);
  if (scenario === 'sha-no-shan') assert.deepEqual(captured.value.final, {
    targetHp: 3, cardPosition: 'd', inActorHand: false, inDiscard: true, useCount: 1, targetAlive: true,
  });
  if (scenario === 'damage-callback-cancel') {
    assert.deepEqual(captured.value.skillCallbacks.map(({ timing, hp }) => [timing, hp]), [
      ['damage', 3], ['damageCancelled', 3], ['shaDamage', 3], ['useCardAfter', 3],
    ]);
    assert.equal(captured.value.final.targetHp, 3);
  }
  if (isRecast) {
    const { setup, afterDefault, overrideSetup, callbacks, afterOverride } = captured.value;
    assert.equal(setup.material.strict, true);
    assert.equal(setup.control.strict, false);
    assert.equal(afterDefault.material.discard, true);
    assert.equal(afterDefault.control.actorHand, true);
    assert.equal(afterOverride.material.targetHand, true);
    assert.deepEqual(callbacks.map(callback => callback.kind), ['lose', 'gain']);
    for (const [before, after] of [[setup, afterDefault], [overrideSetup, afterOverride]]) {
      assert.equal(after.materialLocations, 1);
      assert.equal(after.replacementLocations, 1);
      assert.equal(after.replacement.actorHand, true);
      assert.equal(after.handSize, before.handSize);
      assert.equal(after.useCardStat, before.useCardStat);
      assert.equal(after.targetLinked, setup.targetLinked);
      assert.equal(after.actorLinked, setup.actorLinked);
    }
  }
  result = {
    schema: 'noname-reference-trace-v1',
    referenceCommit: commit,
    mode: 'identity',
    scenario: descriptions.get(scenario),
    ...captured.value,
  };
  if (argv[0] === '--check') {
    const expected = JSON.parse(await readFile(argv[1], 'utf8'));
    assert.deepEqual(result, expected, 'Reference trace does not match the golden file');
  }
} finally {
  await context.close();
  await browser.close();
}

function formatResult(value) {
  const marker = '__REFERENCE_EVENTS__';
  const compactEvents = value.events.map(event => `    ${JSON.stringify(event)}`).join(',\n');
  const compactSupplement = (rows, indentation) => `[\n${rows.map(row => `${' '.repeat(indentation)}${JSON.stringify(row)}`).join(',\n')}\n${' '.repeat(indentation - 2)}]`;
  const output = { ...value, events: marker };
  if (value.gushe) output.gushe = { ...value.gushe, stages: '__REFERENCE_GUSHE_STAGES__' };
  if (value.earlyRevival) output.earlyRevival = { ...value.earlyRevival, stages: '__REFERENCE_EARLY_STAGES__' };
  let formatted = JSON.stringify(output, null, 2).replace(`"${marker}"`, `[\n${compactEvents}\n  ]`);
  if (value.gushe) formatted = formatted.replace('"__REFERENCE_GUSHE_STAGES__"', compactSupplement(value.gushe.stages, 6));
  if (value.earlyRevival) formatted = formatted.replace('"__REFERENCE_EARLY_STAGES__"', compactSupplement(value.earlyRevival.stages, 6));
  return formatted + '\n';
}

if (argv[0] === '--check') {
  console.log(`Reference trace matches ${argv[1]}`);
} else if (argv[0] === '--write') {
  await writeFile(argv[1], formatResult(result));
  console.log(`Wrote reference trace to ${argv[1]}`);
} else {
  process.stdout.write(formatResult(result));
}
