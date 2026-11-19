// Run against Vite with an independent qa-revival WASM build.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const { chromium } = createRequire(import.meta.url)(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_REVIVAL_BINDINGS
assert.ok(bindings, 'Set MAK_REVIVAL_BINDINGS to the qa-revival wasm-bindgen output')
const fixture = await fs.readFile(path.join(bindings, 'duel.js'), 'utf8')
const url = process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'
const evidence = process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence) {
  const root = fileURLToPath(new URL('../../', import.meta.url))
  assert.ok(!path.resolve(evidence, '.').startsWith(root) && evidence !== path.resolve(root))
  await fs.mkdir(evidence, { recursive: true })
}
const errors = []
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const button = (page, name) => page.getByRole('button', { name, exact: true })
const seat = page => page.locator('[data-seat="1"]')
const view = (page, viewer = 0) => page.evaluate(id => JSON.parse(window.revivalDuel.view(id)), viewer)

async function start(withRules) {
  const page = await browser.newPage({ locale: 'zh-CN', viewport: { width: 1280, height: 720 } })
  page.on('pageerror', error => errors.push(error.message))
  page.on('console', message => {
    if (['error', 'warning'].includes(message.type()) && !message.location().url.endsWith('/favicon.ico')) {
      errors.push(message.text())
    }
  })
  if (withRules) {
    await page.route('**/src/generated/duel/duel.js*', route => route.fulfill({
      contentType: 'text/javascript',
      body: fixture + '\nDuel.classic = (...args) => (window.revivalDuel = classicRevivalFixture(...args))',
    }))
    await page.route('**/src/generated/duel/duel_bg.wasm*', route => route.fulfill({
      contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm'),
    }))
  }
  await page.clock.install({ time: new Date('2026-01-01T12:00:00Z') })
  await page.clock.pauseAt(new Date('2026-01-01T12:01:00Z'))
  await page.addInitScript(() => {
    Math.random = () => 317.1 / 0x7fffffff
    localStorage.setItem('mak-reduced-motion', 'true')
  })
  await page.goto(url)
  await button(page, '开始游玩').click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  await button(page, '关羽').click()
  await page.locator('select').selectOption('8')
  await button(page, '开始对局').click()
  await page.waitForURL('**/battle')
  await page.clock.runFor(1000)
  assert.equal(await page.locator('[data-seat]').count(), 8)
  assert.equal(await page.locator('vite-error-overlay').count(), 0)
  return page
}

async function target(page, label) {
  const before = await view(page)
  await button(page, label).click()
  assert.ok(await button(page, '确认').isDisabled())
  await seat(page).locator('.seat-select').click()
  await button(page, '确认').click()
  await page.clock.runFor(1000)
  const after = await view(page)
  assert.equal(after.version, before.version + 1)
  return after
}

async function answer(page, label) {
  assert.deepEqual((await view(page)).choice.branches.map(b => b.label), ['允许复活', '取消复活'])
  await button(page, label).click()
  await button(page, '确认').click()
  await page.clock.runFor(1000)
  return view(page)
}

try {
  const normal = await start(false)
  assert.equal(await normal.evaluate(async () => 'classicRevivalFixture' in await import('/src/generated/duel/duel.js')), false)
  assert.equal(await button(normal, '令其阵亡').count(), 0)
  await normal.close()

  const page = await start(true)
  const original = (await view(page, 1)).seats[1]
  const handIds = original.hand.map(c => c.id)
  assert.ok(handIds.length > 0)
  assert.equal((await view(page)).seats[1].role, null)
  let state = await target(page, '令其阵亡')
  assert.equal(state.seats[1].alive, false)
  assert.equal(state.seats[1].role, original.role)
  assert.ok((await seat(page).locator('.seat-identity').innerText()).startsWith('阵亡'))
  assert.ok(handIds.every(id => state.discard.some(c => c.id === id)))
  if (evidence) await page.screenshot({ path: path.join(evidence, 'revival-death.png') })

  state = await target(page, '发动复活')
  assert.equal(state.seats[1].alive, false)
  state = await answer(page, '取消复活')
  assert.equal(state.seats[1].alive, false)
  assert.ok(state.events.some(e => e.type === 'revive' && e.status === 'cancelled'))
  await target(page, '发动复活')
  state = await answer(page, '允许复活')
  assert.deepEqual([state.seats[1].alive, state.seats[1].hp, state.seats[1].maxHp], [true, 1, 4])
  assert.ok(state.events.some(e => e.type === 'revive' && e.hp === 1 && e.status === 'finished'))

  await target(page, '令其阵亡')
  state = await target(page, '直接复活')
  assert.deepEqual([state.seats[1].alive, state.seats[1].hp, state.seats[1].role, state.seats[1].hero],
    [true, 2, original.role, original.hero])
  assert.equal(state.seats[1].handCount, 0)
  assert.ok(handIds.every(id => state.discard.some(c => c.id === id)))
  assert.ok(!(await seat(page).locator('.seat-identity').innerText()).includes('阵亡'))
  assert.ok(await seat(page).locator('[aria-label="体力 2 / 4"]').isVisible())
  await page.setViewportSize({ width: 640, height: 272 })
  if (evidence) await page.screenshot({ path: path.join(evidence, 'revival-return.png') })

  await button(page, '结束出牌').click()
  await page.clock.runFor(500)
  state = await view(page)
  if (state.choice?.kind === 'discard') {
    for (const id of state.choice.cards.slice(0, state.choice.min)) {
      await page.locator(`[data-hand] [data-card-id="${id}"]`).click()
    }
    await button(page, '确认').click()
    await page.clock.runFor(200)
  }
  state = await view(page)
  assert.equal(state.turn.seat, 1, 'the revived physical neighbour takes the next natural turn')
  assert.ok(state.seats[1].handCount >= 2)
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, originalHand: handIds, role: original.role, nextTurn: state.turn, evidence }))
} finally {
  await browser.close()
}
