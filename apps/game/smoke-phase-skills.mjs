// Run against Vite with a separate duel-web build using the non-default qa-phase feature.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_PHASE_BINDINGS
assert.ok(bindings, 'Set MAK_PHASE_BINDINGS to the qa-phase wasm-bindgen output')
const url = process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'
const fixtureJs = await fs.readFile(path.join(bindings, 'duel.js'), 'utf8')
const evidence = process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')
  assert.ok(evidence !== root && !evidence.startsWith(root + path.sep), 'Save screenshots outside the repo')
  await fs.mkdir(evidence, { recursive: true })
}
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const errors = []
const button = (page, name) => page.getByRole('button', { name, exact: true })
const seat = (page, id) => page.locator(`[data-seat="${id}"]`)
const card = (page, id) => page.locator(`[data-hand] [data-card-id="${id}"]`)
const view = (page, id = 0) => page.evaluate((viewer) => JSON.parse(window.phaseDuel.view(viewer)), id)
const shot = (page, name) => evidence && page.screenshot({ path: path.join(evidence, `phase-${name}.png`) })

async function start(fixture, seed = 317, scenario = "dangxian") {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type())) errors.push(`${message.location().url} ${message.text()}`)
  })
  if (fixture) {
    const factory = `Duel.classic = (...args) => (window.phaseDuel = classicPhaseSkillsFixture(...args, "${scenario}"))`
    await page.route('**/src/generated/duel/duel.js*', (route) =>
      route.fulfill({ contentType: 'text/javascript', body: `${fixtureJs}\n${factory}` }))
    await page.route('**/src/generated/duel/duel_bg.wasm*', (route) =>
      route.fulfill({ contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm') }))
  }
  await page.clock.install({ time: new Date('2026-01-01T12:00:00Z') })
  await page.clock.pauseAt(new Date('2026-01-01T12:01:00Z'))
  await page.addInitScript((seed) => {
    Math.random = () => (seed + 0.1) / 0x7fffffff
    localStorage.setItem('mak-reduced-motion', 'true')
  }, seed)
  await page.goto(url)
  await button(page, '开始游玩').click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  assert.equal(await page.locator('.pick-grid button').count(), 25)
  await button(page, '关羽').click()
  await page.locator('select').selectOption('8')
  await button(page, '开始对局').click()
  await page.waitForURL('**/battle')
  assert.equal(await page.locator('[data-seat]').count(), 8)
  assert.equal(await page.locator('vite-error-overlay').count(), 0)
  await page.clock.runFor(1000)
  return page
}

try {
  const normal = await start(false)
  assert.equal(await normal.evaluate(async () => Object.hasOwn(await import('/src/generated/duel/duel.js'), 'classicPhaseSkillsFixture')), false)
  assert.equal(await normal.locator('[data-phase-reason], [data-turn-reason], .classic-seat-mark').count(), 0)
  assert.equal(await button(normal, '发动放权').count(), 0)
  await normal.close()

  const d = await start(true)
  let state = await view(d)
  const initialHp = state.seats[1].hp
  assert.equal(state.phaseReason, 'dangxian')
  assert.match(await d.locator('[data-phase-reason]').innerText(), /当先/)
  await shot(d, 'dangxian-extra')
  await card(d, 104).click(); await seat(d, 1).locator('.seat-select').click(); await button(d, '确认').click()
  await d.clock.runFor(10000); state = await view(d)
  assert.equal(state.seats[1].hp, initialHp - 1)
  assert.ok(!state.actions.some((a) => a.type === 'playCard' && a.card === 105))
  await button(d, '结束出牌').click(); await d.clock.runFor(10000); state = await view(d)
  assert.equal(state.turn.phase, 'play'); assert.equal(state.phaseReason, undefined)
  assert.equal(await d.locator('[data-phase-reason]').count(), 0)
  assert.ok(state.actions.some((a) => a.type === 'playCard' && a.card === 105 && a.targets.join() === '1'))
  await card(d, 105).click(); await seat(d, 1).locator('.seat-select').click(); await button(d, '确认').click()
  await d.clock.runFor(10000); state = await view(d)
  assert.equal(state.seats[1].hp, initialHp - 2)
  assert.ok(state.discard.some((c) => c.id === 104) && state.discard.some((c) => c.id === 105))
  await d.close()

  const declined = await start(true, 317, 'fangquan')
  await button(declined, '不发动放权').click(); await button(declined, '确认').click()
  await declined.clock.runFor(3000)
  state = await view(declined)
  assert.equal(state.turn.phase, 'play'); assert.equal(state.choice, null)
  assert.ok(state.actions.some((a) => a.type === 'endPlay'))
  assert.deepEqual(state.seats[0].marks ?? [], [])
  await declined.close()

  const f = await start(true, 317, 'fangquan')
  assert.equal(await f.locator('[data-seat]').count(), 8)
  assert.equal(await f.locator('.classic-opponents [data-seat]').count(), 7)
  for (let id = 1; id < 8; id++) assert.equal((await view(f, id)).choice, null)
  await button(f, '发动放权').click(); await button(f, '确认').click(); await f.clock.runFor(3000)
  state = await view(f)
  assert.equal(state.turn.phase, 'discard')
  assert.deepEqual(state.seats[0].marks, [{ skill: 'fangquan2', count: 1, source: null }])
  assert.match(await seat(f, 0).locator('[data-mark="fangquan2"]').innerText(), /放权 ×1/)
  assert.equal(state.choice.kind, 'discard')
  await card(f, state.choice.cards[0]).click(); await button(f, '确认').click(); await f.clock.runFor(3000)
  state = await view(f)
  assert.equal(state.turn.phase, 'end')
  const cost = state.choice.branches.find((b) => b.label === '弃置一张手牌').cards[0]
  assert.ok(state.seats[0].hand.some((c) => c.id === cost))
  await button(f, '弃置一张手牌').click(); await card(f, cost).click(); await button(f, '确认').click()
  await f.clock.runFor(1500); state = await view(f)
  assert.ok(state.discard.some((c) => c.id === cost))
  assert.ok(!state.seats[0].hand.some((c) => c.id === cost))
  assert.deepEqual(state.choice.branches[0].targets, [1, 2, 3, 4, 5, 6, 7])
  for (let id = 1; id < 8; id++) assert.equal((await view(f, id)).choice, null)
  await button(f, '选择额外回合角色').click(); await seat(f, 1).locator('.seat-select').click()
  await button(f, '确认').click(); await f.clock.runFor(5000); state = await view(f)
  assert.equal(state.turn.seat, 1); assert.equal(state.turn.number, 2); assert.equal(state.turnReason, 'fangquan2')
  assert.deepEqual(state.seats[1].marks, [{ skill: 'fangquan', count: 1, source: 0 }])
  assert.match(await f.locator('[data-turn-reason]').innerText(), /放权额外回合/)
  assert.match(await seat(f, 1).locator('[data-mark="fangquan"]').innerText(), /放权 ×1 · 来源 关羽/)
  await f.setViewportSize({ width: 844, height: 390 })
  const markBox = await seat(f, 1).locator('[data-mark="fangquan"]').boundingBox()
  assert.ok(markBox.x >= 0 && markBox.x + markBox.width <= 844 && markBox.y >= 0 && markBox.y + markBox.height <= 390)
  await shot(f, 'extra-turn-landscape')
  await f.setViewportSize({ width: 1280, height: 720 })
  await button(f, '继续当前回合').click(); await button(f, '确认').click()
  for (let i = 0; i < 60; i++) {
    await f.clock.runFor(3000); state = await view(f)
    if (state.turn.number === 3) break
    if (state.waitingFor !== 0 || !state.choice) continue
    if (state.choice.kind === 'shan' || state.choice.kind === 'save') {
      if (await button(f, '放弃').isDisabled()) { await f.clock.runFor(5000); continue }
      if (state.choice.cards.length) { await card(f, state.choice.cards[0]).click(); await button(f, '确认').click() }
      else await button(f, '放弃').click()
    } else throw new Error(`Unexpected choice: ${state.choice.kind}`)
  }
  assert.equal(state.turn.number, 3); assert.equal(state.turn.seat, 1)
  assert.equal(state.turnReason, undefined)
  assert.deepEqual(state.seats[1].marks ?? [], [])
  assert.equal(await f.locator('[data-turn-reason], [data-mark="fangquan"]').count(), 0)
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, dangxianCards: [104, 105], paid: cost, extraSeat: 1, naturalSeat: state.turn.seat, errors, evidence }))
} finally { await browser.close() }
