// Run against Vite with a separate duel-web build using the non-default qa-zhongyi feature.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_ZHONGYI_BINDINGS
assert.ok(bindings, 'Set MAK_ZHONGYI_BINDINGS to the qa-zhongyi wasm-bindgen output')
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
const view = (page, id = 0) => page.evaluate((viewer) => JSON.parse(window.zhongyiDuel.view(viewer)), id)
const shot = (page, name) => evidence && page.screenshot({ path: path.join(evidence, `zhongyi-${name}.png`) })

async function start(fixture, seed = 317) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type())) errors.push(`${message.location().url} ${message.text()}`)
  })
  if (fixture) {
    const factory = 'Duel.classic = (...args) => (window.zhongyiDuel = classicZhongyiFixture(...args))'
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
  assert.equal(await normal.evaluate(async () => Object.hasOwn(await import('/src/generated/duel/duel.js'), 'classicZhongyiFixture')), false)
  assert.equal(await button(normal, '忠义').count(), 0)
  assert.equal(await normal.locator('.classic-limited, .classic-expansions').count(), 0)
  const native = await normal.evaluate(async () => {
    const mod = await import('/src/generated/duel/duel.js')
    await mod.default()
    const duel = mod.Duel.classic(317, 0, 'guanyu', 8)
    try { return JSON.parse(duel.view(0)).virtualPlays } finally { duel.free() }
  })
  assert.ok(native.some((play) => play.kind === 'sha'), 'ordinary Guan Yu retains Wusheng')
  await normal.close()

  // The equipment zone is offered through the same branch card picker as hand cards.
  const equipped = await start(true, 12)
  await card(equipped, 54).click(); await button(equipped, '确认').click(); await equipped.clock.runFor(1800)
  assert.ok((await view(equipped)).seats[0].equipment.some((c) => c.id === 54))
  await button(equipped, '忠义').click()
  const equipChoice = card(equipped, 54)
  assert.ok(await equipChoice.isVisible())
  await equipChoice.click(); await button(equipped, '确认').click(); await equipped.clock.runFor(1800)
  assert.deepEqual((await view(equipped)).seats[0].expansions, [{ card: { id: 54, kind: 'bagua' }, tags: ['zhongyi2'] }])
  assert.ok(!(await view(equipped)).seats[0].equipment.some((c) => c.id === 54))
  await equipped.close()

  const page = await start(true)
  const initial = await view(page)
  const material = 28
  assert.equal(initial.activePlays[0].skill, 'zhongyi')
  await button(page, '忠义').click()
  assert.ok((await card(page, material).isEnabled()))
  await card(page, material).click(); await button(page, '确认').click(); await page.clock.runFor(1800)
  const active = await view(page)
  assert.deepEqual(active.seats[0].expansions, [{ card: { id: material, kind: 'sha' }, tags: ['zhongyi2'] }])
  assert.deepEqual(active.seats[0].limitedSpent, ['zhongyi'])
  assert.deepEqual(active.activePlays ?? [], [])
  assert.equal(await button(page, '忠义').count(), 0)
  for (let id = 1; id < 8; id++) {
    const observer = await view(page, id)
    assert.deepEqual(observer.seats[0].expansions, active.seats[0].expansions)
    assert.deepEqual(observer.seats[0].limitedSpent, ['zhongyi'])
    assert.ok(observer.cardFaces.some((face) => face.id === material))
  }
  assert.match(await seat(page, 0).locator('.classic-limited').innerText(), /忠义/)
  const expansion = seat(page, 0).locator('.classic-expansions')
  assert.match(await expansion.locator('summary').innerText(), /扩展牌 1/)
  await expansion.locator('summary').click()
  assert.match(await expansion.innerText(), /忠义 \(zhongyi2\)/)
  assert.ok(await expansion.locator('img').evaluate((img) => img.complete && img.naturalWidth > 0))
  await shot(page, 'expansion')
  await page.setViewportSize({ width: 844, height: 390 })
  assert.ok(await expansion.locator(`[data-expansion-card="${material}"]`).isVisible())
  const box = await expansion.locator(`[data-expansion-card="${material}"]`).boundingBox()
  assert.ok(box.x >= 0 && box.x + box.width <= 844 && box.y >= 0 && box.y + box.height <= 390)
  await shot(page, 'landscape')
  await expansion.locator('summary').click()
  await page.setViewportSize({ width: 1280, height: 720 })

  // The owner is its own teammate: a real Sha deals two damage through the registered trigger.
  await card(page, 11).click(); await seat(page, 1).locator('.seat-select').click()
  await button(page, '确认').click(); await page.clock.runFor(12000)
  let state = await view(page)
  assert.equal(state.seats[1].hp, initial.seats[1].hp - 2)
  assert.ok(state.events.some((event) => event.type === 'damage' && event.status === 'finished' && event.amount === 2))
  await button(page, '结束出牌').click()
  for (let i = 0; i < 100; i++) {
    await page.clock.runFor(3000); state = await view(page)
    if (state.turn.round > 1 || state.winner || state.drawn) break
    if (state.waitingFor !== 0 || !state.choice) continue
    const choice = state.choice
    if (choice.kind === 'shan' || choice.kind === 'save') {
      if (await button(page, '放弃').isDisabled()) { await page.clock.runFor(8000); continue }
      if (choice.cards.length === 1) await button(page, choice.kind === 'shan' ? '使用闪' : '使用桃').click()
      else if (choice.cards.length > 1) { await card(page, choice.cards[0]).click(); await button(page, '确认').click() }
      else await button(page, '放弃').click()
    } else if (choice.kind === 'discard') {
      for (const id of choice.cards.slice(0, choice.min)) await card(page, id).click()
      await button(page, '确认').click()
    } else if (choice.kind === 'content') {
      const branch = choice.branches[0]
      await button(page, branch.label).click()
      if ((await view(page)).version !== state.version) continue
      for (const id of branch.cards.slice(0, branch.minCards)) {
        if (await card(page, id).count()) await card(page, id).click()
        else await page.locator(`.classic-pool [data-card-id="${id}"]`).click()
      }
      for (const id of branch.targets.slice(0, branch.minTargets))
        await seat(page, id).locator(id === 0 ? '.self-select' : '.seat-select').click()
      await button(page, '确认').click()
    } else throw new Error(`Unexpected choice: ${choice.kind}`)
  }
  assert.ok(state.turn.round > 1, 'the ordinary table reaches the next round')
  assert.deepEqual(state.seats[0].expansions ?? [], [])
  assert.ok(state.discard.some((item) => item.id === material))
  assert.deepEqual(state.seats[0].limitedSpent, ['zhongyi'])
  assert.deepEqual(state.activePlays ?? [], [])
  assert.equal(await seat(page, 0).locator('.classic-expansions').count(), 0)
  assert.match(await seat(page, 0).locator('.classic-limited').innerText(), /忠义/)
  await shot(page, 'expired')
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, material, equipmentSeed: 12, equipment: 54, round: state.turn.round, errors, evidence }))
} finally { await browser.close() }
