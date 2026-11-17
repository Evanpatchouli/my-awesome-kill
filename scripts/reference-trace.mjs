import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { installTriggerOrderScenario } from './reference-trigger-order-cancel.mjs';

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
let scenario = 'sha-no-shan';
const scenarioIndex = argv.indexOf('--scenario');
if (scenarioIndex !== -1) {
  scenario = argv[scenarioIndex + 1];
  argv.splice(scenarioIndex, 2);
}
if (!['sha-no-shan', 'trigger-order-cancel'].includes(scenario)) throw new Error(`Unknown scenario: ${scenario}`);
if (argv.length && (argv.length !== 2 || !['--check', '--write'].includes(argv[0]))) {
  throw new Error('Usage: node scripts/reference-trace.mjs [--check|--write path]');
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

  await context.addInitScript({ content: `window.__installTriggerOrderScenario = ${installTriggerOrderScenario.toString()};` });

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
    for (const skill of ['reference_fixture_setup', 'reference_fixture_use']) {
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
  if (scenario === 'sha-no-shan') assert.deepEqual(captured.value.final, {
    targetHp: 3, cardPosition: 'd', inActorHand: false, inDiscard: true, useCount: 1, targetAlive: true,
  });
  result = {
    schema: 'noname-reference-trace-v1',
    referenceCommit: commit,
    mode: 'identity',
    scenario: scenario === 'sha-no-shan' ? 'ordinary physical sha against an adjacent AI target with no shan' : 'damage trigger ordering, cost, and cancellation for physical sha',
    ...captured.value,
  };
} finally {
  await browser.close();
}

function formatResult(value) {
  const marker = '__REFERENCE_EVENTS__';
  const compactEvents = value.events.map(event => `    ${JSON.stringify(event)}`).join(',\n');
  return JSON.stringify({ ...value, events: marker }, null, 2)
    .replace(`"${marker}"`, `[\n${compactEvents}\n  ]`) + '\n';
}

if (argv[0] === '--check') {
  const expected = JSON.parse(await readFile(argv[1], 'utf8'));
  assert.deepEqual(result, expected, 'Reference trace does not match the golden file');
  console.log(`Reference trace matches ${argv[1]}`);
} else if (argv[0] === '--write') {
  await writeFile(argv[1], formatResult(result));
  console.log(`Wrote reference trace to ${argv[1]}`);
} else {
  process.stdout.write(formatResult(result));
}
