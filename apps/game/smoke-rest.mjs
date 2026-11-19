// Run against Vite with an independent qa-rest WASM build.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const { chromium } = createRequire(import.meta.url)(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_REST_BINDINGS
assert.ok(bindings, 'Set MAK_REST_BINDINGS to the qa-rest wasm-bindgen output')
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
const view = (page, viewer = 0) => page.evaluate(id => JSON.parse(window.restDuel.view(id)), viewer)

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
      body: fixture + '\nDuel.classic = (...args) => (window.restDuel = classicRestFixture(...args))',
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


async function target(page, label, id = 1) {
  const before = await view(page)
  await button(page, label).click()
  assert.ok(await button(page, '确认').isDisabled())
  const targetSeat = page.locator(`[data-seat="${id}"] .seat-select`)
  await targetSeat.click()
  assert.ok(await button(page, '确认').isEnabled())
  await targetSeat.click()
  assert.ok(await button(page, '确认').isDisabled())
  await targetSeat.click()
  await button(page, '确认').click()
  await page.clock.runFor(1000)
  const after = await view(page)
  assert.equal(after.version, before.version + 1)
  return after
}
try {
  const normal = await start(false)
  assert.equal(await normal.evaluate(async () => 'classicRestFixture' in await import('/src/generated/duel/duel.js')), false)
  assert.equal(await button(normal, '永久休整').count(), 0)
  await normal.close()
  const page = await start(true)
  await target(page, '公开赠牌', 2)
  const original = (await view(page, 2)).seats[2]
  const handIds = original.hand.map(c => c.id)
  const knownBefore = (await view(page)).seats[2].knownHand
  assert.ok(knownBefore?.length > 0, 'the transferred card is publicly known')
  let state = await target(page, '永久休整', 2)
  assert.deepEqual(state.seats[2].rest, { unit: 'phase', count: -1 })
  assert.equal(state.seats[2].alive, true)
  assert.deepEqual((await view(page, 2)).seats[2].hand.map(c => c.id), handIds)
  assert.deepEqual(state.seats[2].knownHand, knownBefore)
  assert.equal(state.seats[2].hero, original.hero)
  assert.equal(state.seats[2].role, null)
  assert.match(await page.locator('[data-seat="2"] [data-rest-seat]').innerText(), /休整 · 无限期/)
  await page.setViewportSize({ width: 640, height: 272 })
  if (evidence) await page.screenshot({ path: path.join(evidence, 'rest-permanent.png') })
  state = await target(page, '结束休整', 2)
  assert.equal(state.seats[2].rest, undefined)
  assert.deepEqual(state.seats[2].knownHand, knownBefore)
  assert.deepEqual((await view(page, 2)).seats[2].hand.map(c => c.id), handIds)
  state = await target(page, '休整一轮', 2)
  assert.deepEqual(state.seats[2].rest, { unit: 'round', count: 1 })
  assert.match(await page.locator('[data-seat="2"] [data-rest-seat]').innerText(), /休整 · 余 1 轮/)
  if (evidence) await page.screenshot({ path: path.join(evidence, 'rest-round.png') })
  await target(page, '结束休整', 2)
  await page.setViewportSize({ width: 1280, height: 720 })
  const mowangBefore = (await view(page, 1)).seats[1]
  state = await target(page, '令其阵亡', 1)
  assert.equal(state.seats[1].alive, true)
  assert.deepEqual(state.seats[1].rest, { unit: 'round', count: 1 })
  assert.match(await page.locator('[data-seat="1"] [data-rest-seat]').innerText(), /休整/)
  assert.equal(state.seats[1].hero, mowangBefore.hero)
  await button(page, '结束出牌').click()
  await page.clock.runFor(1)
  state = await view(page)
  if (state.choice?.kind === 'discard') {
    for (const id of state.choice.cards.slice(0, state.choice.min)) {
      await page.locator(`[data-hand] [data-card-id="${id}"]`).click()
    }
    await button(page, '确认').click()
    await page.clock.runFor(200)
  }
  state = await view(page)
  assert.equal(state.turn.seat, 1, 'returning physical neighbour takes its natural turn')
  assert.equal(state.seats[1].rest, undefined)
  assert.ok(state.seats[1].handCount >= 3, 'return draw and natural turn draw both occur')
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, preservedHand: handIds, nextTurn: state.turn, returnHand: state.seats[1].handCount, evidence }))
  const natural = await start(true)
  await button(natural, '结束出牌').click()
  for (let i = 0; i < 100; i++) {
    await natural.clock.runFor(100)
    const snapshot = await view(natural)
    if (snapshot.seats[1].rest) {
      assert.deepEqual(snapshot.seats[1].rest, { unit: 'round', count: 1 })
      assert.equal(snapshot.seats[1].alive, true)
      assert.match(await natural.locator('[data-seat="1"] [data-rest-seat]').innerText(), /休整 · 余 1 轮/)
      if (evidence) await natural.screenshot({ path: path.join(evidence, 'rest-natural-death.png') })
      console.log(JSON.stringify({ naturalDeath: true, turn: snapshot.turn, step: i }))
      break
    }
    if (snapshot.choice?.kind === 'discard') {
      await natural.clock.runFor(1000)
      for (const id of snapshot.choice.cards.slice(0, snapshot.choice.min)) await natural.locator(`[data-hand] [data-card-id="${id}"]`).click()
      await button(natural, '确认').click()
      continue
    }
    if (snapshot.choice?.kind === 'content' || snapshot.choice?.kind === 'shan' || snapshot.choice?.kind === 'save') {
      await natural.clock.runFor(1000)
      const choice = (await view(natural)).choice
      if (!choice || choice.seat !== 0) continue
      if (await button(natural, '放弃').isDisabled()) continue
      if (choice.kind === 'shan' || choice.kind === 'save') {
        await button(natural, '放弃').click({timeout: 2000})
      } else if (choice.branches?.some(branch => branch.label === '放弃')) {
        await button(natural, '放弃').click({timeout: 2000})
        await button(natural, '确认').click()
      } else {
        throw new Error(`Unexpected choice: ${JSON.stringify(choice)}`)
      }
      continue
    }
    if (snapshot.choice) {
      console.log(JSON.stringify({ pendingChoice: snapshot.choice, turn: snapshot.turn, step: i }))
      throw new Error('natural death run awaits a human choice')
    }
    if (i === 99) throw new Error('natural Mowang death did not occur')
  }

} finally {
  await browser.close()
}
