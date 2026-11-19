// Run against Vite with separate duel-web bindings built with --features qa-vitality.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_VITALITY_BINDINGS
assert.ok(bindings, 'Set MAK_VITALITY_BINDINGS to the qa-vitality wasm-bindgen output')
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
const view = (page, seat = 0) => page.evaluate((id) => JSON.parse(window.vitalityDuel.view(id)), seat)
const shot = (page, name) => evidence && page.screenshot({ path: path.join(evidence, `vitality-${name}.png`) })

async function start(fixture) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type()) && !message.location().url.endsWith('/favicon.ico')) errors.push(message.text())
  })
  if (fixture) {
    await page.route('**/src/generated/duel/duel.js*', (route) => route.fulfill({ contentType: 'text/javascript', body: `${fixtureJs}\nDuel.classic = (...args) => (window.vitalityDuel = classicVitalityFixture(...args))` }))
    await page.route('**/src/generated/duel/duel_bg.wasm*', (route) => route.fulfill({ contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm') }))
  }
  await page.clock.install({ time: new Date('2026-01-01T12:00:00Z') })
  await page.clock.pauseAt(new Date('2026-01-01T12:01:00Z'))
  await page.addInitScript(() => { Math.random = () => 317.1 / 0x7fffffff; localStorage.setItem('mak-reduced-motion', 'true') })
  await page.goto(url, { waitUntil: 'domcontentloaded' })
  await button(page, '开始游玩').click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  await button(page, '关羽').click()
  await page.locator('select').selectOption('8')
  await button(page, '开始对局').click()
  await page.waitForURL('**/battle')
  await page.clock.runFor(1000)
  assert.equal(await page.locator('[data-seat]').count(), 8)
  return page
}

try {
  const normal = await start(false)
  assert.equal(await normal.evaluate(async () => Object.hasOwn(await import('/src/generated/duel/duel.js'), 'classicVitalityFixture')), false)
  assert.equal(await button(normal, '增加体力上限').count(), 0)
  assert.equal(await button(normal, '减少体力上限').count(), 0)
  await normal.close()

  const page = await start(true)
  let state = await view(page)
  assert.equal(state.turn.seat, 0)
  assert.deepEqual([state.seats[1].hp, state.seats[1].maxHp, state.seats[1].handCount], [4, 4, 4])
  assert.ok(state.seats.every((seat) => !seat.alive || seat.handCount >= 4))
  for (const [label, type, hp, maxHp, amount] of [
    ['增加体力上限', 'gainMaxHp', 4, 5, 2],
    ['减少体力上限', 'loseMaxHp', 4, 4, 1],
    ['减少体力上限', 'loseMaxHp', 3, 3, 1],
  ]) {
    const before = state
    const beforeOwn = await view(page, 1)
    const beforeIds = beforeOwn.seats[1].hand.map((card) => card.id)
    await button(page, label).click()
    assert.ok(await button(page, '确认').isDisabled())
    await page.locator('[data-seat="1"] .seat-select').click()
    assert.ok(await button(page, '确认').isEnabled())
    await button(page, '确认').click()
    await page.clock.runFor(1000)
    state = await view(page)
    assert.equal(state.version, before.version + 1)
    assert.deepEqual([state.seats[1].hp, state.seats[1].maxHp], [hp, maxHp])
    assert.equal(state.seats[1].handCount, before.seats[1].handCount + amount)
    assert.equal(await page.locator(`[data-seat="1"] .health[aria-label="体力 ${hp} / ${maxHp}"]`).count(), 1)
    const own = await view(page, 1)
    const drawn = own.seats[1].hand.filter((card) => !beforeIds.includes(card.id)).map((card) => card.id)
    assert.equal(drawn.length, amount)
    for (const viewer of [0, 2, 3, 4, 5, 6, 7]) {
      const other = await view(page, viewer)
      assert.equal(other.seats[1].hand, null)
      assert.ok(other.cardFaces.every((face) => !drawn.includes(face.id)))
    }
    const previous = new Set(before.events.map((event) => `${event.id}:${event.status}`))
    const events = state.events.filter((event) => !previous.has(`${event.id}:${event.status}`))
    const roots = events.filter((event) => event.type === type && event.status === 'finished' && event.seat === 1)
    assert.equal(roots.length, 1)
    const root = roots[0]
    assert.deepEqual([root.source, root.num, root.before, root.after], [0, 1, before.seats[1].maxHp, maxHp])
    assert.ok(events.some((event) => event.id === root.id && event.status === 'started'))
    const triggers = events.filter((event) => event.type === 'registeredTrigger' && event.seat === 1 && event.status === 'finished')
    assert.equal(triggers.length, 1)
    const draws = events.filter((event) => event.type === 'draw' && event.parent === triggers[0].id && event.status === 'finished')
    assert.equal(draws.length, 1)
    assert.equal(draws[0].amount, amount)
    const gains = events.filter((event) => event.type === 'gain' && event.parent === draws[0].id && event.status === 'finished')
    assert.equal(gains.length, 1)
    assert.equal(gains[0].amount, amount)
    assert.equal(gains[0].card, undefined)
  }
  await page.locator('.classic-log summary').click()
  const lines = await page.locator('.classic-log li').allTextContents()
  for (const change of ['4 → 5', '5 → 4', '4 → 3'])
    assert.ok(lines.some((line) => line.includes(`体力上限 ${change}`)), change)
  await shot(page, 'resolved')
  await page.setViewportSize({ width: 844, height: 390 })
  assert.ok(await page.locator('[data-seat="1"] .health').isVisible())
  await shot(page, 'landscape')
  await page.locator('.classic-log summary').click()
  for (let max = state.seats[1].maxHp; max < 9; max++) {
    await button(page, '增加体力上限').click()
    await page.locator('[data-seat="1"] .seat-select').click()
    await button(page, '确认').click()
    await page.clock.runFor(1000)
  }
  const points = page.locator('[data-seat="1"] .health-point')
  assert.equal(await points.count(), 9)
  const spent = page.locator('[data-seat="1"] .health-point.lost')
  assert.equal(await spent.count(), 6)
  assert.match(await points.first().evaluate(point => getComputedStyle(point).backgroundImage), /hp-full/)
  assert.match(await spent.first().evaluate(point => getComputedStyle(point).backgroundImage), /hp-lost/)
  assert.equal(await page.locator('.game-error, vite-error-overlay').count(), 0)
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, hp: state.seats[1].hp, maxHp: state.seats[1].maxHp, evidence }))
} finally { await browser.close() }
