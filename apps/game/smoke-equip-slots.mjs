import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const { chromium } = createRequire(import.meta.url)(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_EQUIP_SLOTS_BINDINGS
assert.ok(bindings, 'Set MAK_EQUIP_SLOTS_BINDINGS to the qa-equip-slots bindings')
const js = await fs.readFile(path.join(bindings, 'duel.js'), 'utf8')
const evidence = process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence) {
  const root = path.resolve(fileURLToPath(new URL('../../', import.meta.url)))
  assert.ok(evidence !== root && !evidence.startsWith(root + path.sep))
  await fs.mkdir(evidence, { recursive: true })
}
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const errors = []
const page = await browser.newPage({ locale: 'zh-CN', viewport: { width: 1280, height: 720 } })
const button = name => page.getByRole('button', { name, exact: true })
const view = () => page.evaluate(() => JSON.parse(window.equipTable.view(0)))
async function reset() {
  await page.goto(process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420')
  await button('开始游玩').click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  await button('大乔').click()
  await page.locator('select').selectOption('8')
  await button('开始对局').click()
  await page.waitForURL('**/battle')
  await page.clock.runFor(1000)
  assert.equal(await page.locator('[data-seat]').count(), 8)
  assert.equal(await page.locator('vite-error-overlay').count(), 0)
}
async function action(label) {
  await button(label).click()
  await button('确认').click()
  await page.clock.runFor(2000)
}
async function chooseCard(id) {
  await page.locator(`.classic-pool [data-card-id="${id}"]`).click()
  await button('确认').click()
  await page.clock.runFor(2000)
}
try {
  page.on('pageerror', e => errors.push(e.message))
  page.on('console', m => { if (['warning', 'error'].includes(m.type()) && !m.location().url.endsWith('/favicon.ico')) errors.push(m.text()) })
  await page.route('**/src/generated/duel/duel.js*', route => route.fulfill({
    contentType: 'text/javascript', body: js + `
      const originalBot = Duel.prototype.bot;
      Duel.prototype.bot = function () {
        const own = JSON.parse(this.view(1));
        if (localStorage.getItem('qa-combined-mount') === 'true' && own.choice?.seat === 1) {
          const viewers = [...Array(8).keys(), null].map(id => {
            const view = JSON.parse(this.view(id));
            return { viewer: id, hasChoice: !!view.choice, exposesHand: view.seats[1].hand !== null };
          });
          (window.__mountBotChoices ||= []).push({ choice: own.choice, viewers });
        }
        return originalBot.call(this);
      };
      Duel.classic = (...args) => (window.equipTable = localStorage.getItem('qa-combined-mount') === 'true'
        ? classicCombinedMountFixture(...args) : classicEquipSlotsFixture(...args));`,
  }))
  await page.route('**/src/generated/duel/duel_bg.wasm*', route => route.fulfill({ contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm') }))
  await page.clock.install({ time: new Date('2026-01-01T12:00:00Z') })
  await page.clock.pauseAt(new Date('2026-01-01T12:01:00Z'))
  await page.addInitScript(() => { Math.random = () => 317.1 / 0x7fffffff; localStorage.setItem('mak-reduced-motion', 'true') })

  await reset()
  assert.equal((await view()).mountCombined, undefined)
  await action('准备双武器')
  let state = await view()
  assert.deepEqual(state.seats[1].equipment.map(c => c.kind), ['qinggang', 'zhuge'])
  assert.equal(state.seats[1].hand, null)
  await action('废除武器栏')
  state = await view()
  assert.equal(state.choice.seat, 0)
  const candidates = state.choice.branches[0].cards
  assert.equal(candidates.length, 2)
  await button(state.choice.branches[0].label).click()
  await chooseCard(candidates[1])
  state = await view()
  assert.equal(state.seats[1].equipment.length, 1)
  assert.equal(state.discard.filter(c => c.id === candidates[1]).length, 1)
  assert.ok(state.seats[1].equipmentDisabledMarkers.includes('weapon'))
  if (evidence) await page.screenshot({ path: path.join(evidence, 'disabled-slot.png') })

  await reset()
  await action('准备双武器')
  const beforeBot = (await view()).seats[1].equipment
  await action('替换电脑武器')
  state = await view()
  assert.equal(state.choice, null)
  assert.equal(state.seats[1].equipment.length, 2)
  assert.equal(state.seats[1].equipment[0].kind, 'qinglong')
  assert.equal(beforeBot.filter(card => state.discard.some(c => c.id === card.id)).length, 1)
  assert.ok(state.discard.some(card => card.id === beforeBot.find(c => c.kind === 'zhuge').id))
  assert.equal(state.seats[1].hand, null)

  await reset()
  await action('准备自身双武器')
  await action('分离武器登记')
  state = await view()
  assert.deepEqual(state.seats[0].detachedEquipment, ['zhuge'])
  assert.equal(state.seats[0].equipment.length, 1)
  await action('替换自身武器')
  state = await view()
  const options = state.choice.equipmentButtons
  assert.ok(options && options.required === 1 && options.options.length === 2)
  const detached = options.options.find(option => option.kind === 'zhuge')
  const attached = options.options.find(option => option.kind === 'qinggang')
  assert.ok(detached && detached.card === undefined && attached.card !== undefined)
  const detachedChoice = page.locator(`[data-hand-gallery] [data-equipment-button="${detached.id}"]`)
  assert.match(await detachedChoice.innerText(), /效果仍生效/)
  if (evidence) await page.screenshot({ path: path.join(evidence, 'detached-choice.png') })
  await detachedChoice.click()
  await button('确认').click()
  await page.clock.runFor(2000)
  state = await view()
  assert.deepEqual(state.seats[0].detachedEquipment, ['zhuge'])
  assert.equal(state.seats[0].equipment.length, 1)
  assert.ok(!state.seats[0].equipment.some(c => c.kind === 'qinglong'))

  await page.evaluate(() => localStorage.setItem('qa-combined-mount', 'true'))
  await reset()
  state = await view()
  assert.equal(state.mountCombined, true)
  await action('准备合马')
  state = await view()
  assert.deepEqual(state.seats[1].equipment.map(card => card.kind), ['jueying', 'chitu'])
  const initialMounts = state.seats[1].equipment
  await action('替换电脑坐骑')
  state = await view()
  assert.equal(state.seats[1].equipment.length, 2)
  assert.ok(state.seats[1].equipment.some(card => card.kind === 'dawan'))
  const botChoices = await page.evaluate(() => window.__mountBotChoices || [])
  assert.equal(botChoices.length, 1, 'the real B bot resolved one shared-slot choice')
  assert.equal(botChoices[0].choice.branches[0].minCards, 1)
  assert.equal(botChoices[0].choice.branches[0].maxCards, 1)
  assert.deepEqual(new Set(botChoices[0].choice.branches[0].cards), new Set(initialMounts.map(card => card.id)))
  for (const snapshot of botChoices[0].viewers) {
    assert.equal(snapshot.hasChoice, snapshot.viewer === 1, `choice privacy for ${snapshot.viewer}`)
    assert.equal(snapshot.exposesHand, snapshot.viewer === 1, `hand privacy for ${snapshot.viewer}`)
  }
  const third = state.seats[1].equipment.find(card => card.kind === 'dawan')
  const allMounts = [...initialMounts, third]
  assert.equal(initialMounts.filter(card => state.discard.some(discarded => discarded.id === card.id)).length, 1)
  await button('废除合马栏').click()
  await page.locator('[data-seat="1"] .seat-select').click()
  await button('确认').click()
  await page.clock.runFor(2000)
  state = await view()
  assert.deepEqual(state.seats[1].equipment, [])
  assert.deepEqual(state.seats[1].equipmentDisabledMarkers, ['defenseHorse'])
  assert.match(await page.locator('[data-disabled-equipment="1"]').innerText(), /坐骑栏/)
  for (const card of allMounts) assert.equal(state.discard.filter(discarded => discarded.id === card.id).length, 1)
  if (evidence) await page.screenshot({ path: path.join(evidence, 'combined-mount-disabled.png') })
  await button('恢复合马栏').click()
  await page.locator('[data-seat="1"] .seat-select').click()
  await button('确认').click()
  await page.clock.runFor(2000)
  state = await view()
  assert.deepEqual(state.seats[1].equipmentDisabledMarkers ?? [], [])
  assert.equal(await page.locator('[data-disabled-equipment="1"]').count(), 0)
  assert.equal(state.seats[1].hand, null)
  for (const card of allMounts) assert.equal(state.discard.filter(discarded => discarded.id === card.id).length, 1)
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, scenarios: ['human-disable', 'bot-replacement', 'detached-selection', 'combined-mount-bot-and-human'], evidence }))
} finally {
  await browser.close()
}
