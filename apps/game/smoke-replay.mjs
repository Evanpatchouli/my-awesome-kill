// Exercise the default release bindings as an offline, trusted replay host.
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const url = process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const errors = []
const button = (page, name) => page.getByRole('button', { name, exact: true })
const handCard = (page, id) => page.locator(`[data-hand] [data-card-id="${id}"]`)
const read = (page) => page.evaluate(() => {
  const host = window.recordedTable
  return {
    log: JSON.parse(host.log()),
    views: [0, 1, 2, 3, 4, 5, 6, 7, null].map((seat) => JSON.parse(host.view(seat))),
  }
})

try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  const page = await context.newPage()
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type()) && !message.location().url.endsWith('/favicon.ico')) errors.push(message.text())
  })
  // Only replace the constructor at the E2E boundary. Both classes come from the release module.
  await page.route('**/src/generated/duel/duel.js*', async (route) => {
    const response = await route.fetch()
    const source = await response.text()
    await route.fulfill({ response, body: `${source}\nDuel.classic = (seed,human,hero,seats) => (window.recordedTable = new RecordedDuel(JSON.stringify({kind:'seeded',seed,human,hero,seats})))` })
  })
  await page.clock.install({ time: new Date('2026-01-01T12:00:00Z') })
  await page.clock.pauseAt(new Date('2026-01-01T12:01:00Z'))
  await page.addInitScript(() => {
    Math.random = () => 317.1 / 0x7fffffff
    localStorage.setItem('mak-reduced-motion', 'true')
  })
  await page.goto(url, { waitUntil: 'domcontentloaded' })
  assert.match(await page.title(), /随心杀/)
  await button(page, '开始游玩').click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  await button(page, '关羽').click()
  await page.locator('select').selectOption('8')
  await button(page, '开始对局').click()
  await page.waitForURL('**/battle')
  assert.equal(await page.locator('[data-seat]').count(), 8)
  await page.clock.runFor(1000)

  const frames = []
  async function capture(label) {
    const snapshot = await read(page)
    assert.equal(snapshot.views[0].version, snapshot.log.commands.length)
    for (const v of snapshot.views) {
      assert.ok(!Object.hasOwn(v, 'commands') && !Object.hasOwn(v, 'log'), 'the host record is absent from views')
    }
    frames.push({ label, count: snapshot.log.commands.length, views: snapshot.views })
    return snapshot
  }
  let snapshot = await capture('initial')
  assert.equal(snapshot.log.schema, 1)
  assert.deepEqual(snapshot.log.setup, { kind: 'seeded', seed: 317, human: 0, hero: 'guanyu', seats: 8 })
  assert.equal(snapshot.log.commands.length, 0)
  assert.equal(snapshot.views[0].turn.seat, 0)
  const initialHand = snapshot.views[0].seats[0].hand
  assert.ok(initialHand.length > 0)
  assert.deepEqual(await page.locator('[data-hand] [data-card-id]').evaluateAll((nodes) => nodes.map((n) => Number(n.dataset.cardId))), initialHand.map((c) => c.id))
  assert.ok((await handCard(page, initialHand[0].id).getAttribute('aria-label')).length > 0)
  for (const observer of snapshot.views.slice(1)) assert.equal(observer.seats[0].hand, null)

  await button(page, '结束出牌').click()
  snapshot = await capture('private discard')
  const choice = snapshot.views[0].choice
  assert.equal(choice.kind, 'discard')
  assert.equal(snapshot.views[0].waitingFor, 0)
  for (const observer of snapshot.views.slice(1)) {
    assert.equal(observer.choice, null)
    assert.equal(observer.seats[0].hand, null)
  }
  const stale = snapshot.log.commands[0]
  assert.equal(stale.action.type, 'endPlay')
  const rejection = await page.evaluate((command) => {
    const host = window.recordedTable
    const before = host.log()
    let rejected = false
    try { host.submit(JSON.stringify(command)) } catch { rejected = true }
    return { rejected, before, after: host.log() }
  }, stale)
  assert.ok(rejection.rejected, 'an old version is rejected')
  assert.equal(rejection.after, rejection.before, 'a rejected command is not recorded')
  assert.deepEqual((await read(page)).views, snapshot.views)

  await page.clock.runFor(1500)
  for (const id of choice.cards.slice(0, choice.min)) await handCard(page, id).click()
  await button(page, '确认').click()
  snapshot = await capture('discard confirmed')
  assert.equal(snapshot.log.commands.length, 2)
  assert.equal(snapshot.views[0].seats[0].hand.length, initialHand.length - choice.min)
  const botStart = snapshot.log.commands.length
  let lastCount = botStart
  for (let i = 0; i < 400 && snapshot.log.commands.filter((command) => command.seat !== 0).length < 3; i++) {
    await page.clock.runFor(100)
    snapshot = await read(page)
    if (snapshot.log.commands.length > lastCount) {
      await capture(`natural decision ${snapshot.log.commands.length}`)
      lastCount = snapshot.log.commands.length
    }
    const human = snapshot.views[0]
    if (human.waitingFor === 0 && human.choice) {
      await page.clock.runFor(1500)
      if (human.choice.kind === 'shan' || human.choice.kind === 'save') {
        await button(page, '放弃').click()
      } else {
        const pass = human.choice.branches.find((b) => b.id === 0)
        assert.ok(pass, `expected a pass for ${human.choice.kind}`)
        await button(page, pass.label).click()
      }
    }
    if (human.winner || human.drawn) break
  }
  snapshot = await read(page)
  assert.ok(snapshot.log.commands.filter((command) => command.seat !== 0).length >= 3, 'at least three real bot decisions were recorded')
  if (frames.at(-1).count !== snapshot.log.commands.length) await capture('final')
  assert.equal(await page.locator('.game-error, vite-error-overlay').count(), 0)

  // A fresh context has no live host object or intercepted constructor: restore using release bindings.
  const restoredContext = await browser.newContext()
  const restoredPage = await restoredContext.newPage()
  await restoredPage.goto(url, { waitUntil: 'domcontentloaded' })
  const restored = await restoredPage.evaluate(async ({ log, counts }) => {
    const mod = await import('/src/generated/duel/duel.js')
    await mod.default()
    return counts.map((count) => {
      const duel = mod.RecordedDuel.replayAt(JSON.stringify(log), count)
      try { return [0, 1, 2, 3, 4, 5, 6, 7, null].map((seat) => JSON.parse(duel.view(seat))) }
      finally { duel.free() }
    })
  }, { log: snapshot.log, counts: frames.map((frame) => frame.count) })
  for (let i = 0; i < frames.length; i++) assert.deepEqual(restored[i], frames[i].views, `replay ${frames[i].label} at prefix ${frames[i].count}`)
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, frames: frames.map(({ label, count }) => ({ label, count })), commands: snapshot.log.commands.length, botCommands: snapshot.log.commands.filter((c) => c.seat !== 0).length }))
  await restoredContext.close()
  await context.close()
} finally { await browser.close() }
