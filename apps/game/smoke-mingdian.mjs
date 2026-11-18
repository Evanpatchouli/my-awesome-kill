// Run against Vite with separate duel-web bindings built with --features qa-mingdian.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_MINGDIAN_BINDINGS
assert.ok(bindings, 'Set MAK_MINGDIAN_BINDINGS to the qa-mingdian wasm-bindgen output')
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
const favicon404 = new Set()
const button = (page, name) => page.getByRole('button', { name, exact: true })
const card = (page, id) => page.locator(`[data-hand] [data-card-id="${id}"]`)
const view = (page, id = 0) => page.evaluate((viewer) => JSON.parse(window.mingdianDuel.view(viewer)), id)
const shot = (page, name) => evidence && page.screenshot({ path: path.join(evidence, `mingdian-${name}.png`) })

async function start(fixture) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (!['error', 'warning'].includes(message.type())) return
    const source = message.location().url
    if (source === new URL('/favicon.ico', url).href && message.text().includes('404'))
      favicon404.add(source)
    else errors.push(`${source} ${message.text()}`)
  })
  if (fixture) {
    const factory = 'Duel.classic = (...args) => (window.mingdianDuel = classicMingdianFixture(...args))'
    await page.route('**/src/generated/duel/duel.js*', (route) =>
      route.fulfill({ contentType: 'text/javascript', body: `${fixtureJs}\n${factory}` }))
    await page.route('**/src/generated/duel/duel_bg.wasm*', (route) =>
      route.fulfill({ contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm') }))
  }
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

function recastEvents(events, owner, count) {
  const roots = events.filter((event) => event.type === 'recast' && event.status === 'finished' && event.seat === owner)
  assert.equal(roots.length, 1, 'one selection produces one finished recast root')
  const root = roots[0]
  const losses = events.filter((event) => event.type === 'loseToDiscardpile' && event.status === 'finished' && event.parent === root.id)
  assert.equal(losses.length, 1)
  assert.equal(losses[0].amount, count)
  const draws = events.filter((event) => event.type === 'draw' && event.status === 'finished' && event.parent === root.id)
  assert.equal(draws.length, 1, 'the batch has one draw child')
  assert.equal(draws[0].amount, count)
  const gains = events.filter((event) => event.type === 'gain' && event.status === 'finished' && event.parent === draws[0].id)
  assert.equal(gains.length, 1, 'the draw has one gain child')
  assert.equal(gains[0].amount, count)
  assert.equal(gains[0].card, undefined, 'the ordinary gain does not reveal a drawn card')
  return root
}

try {
  const normal = await start(false)
  assert.equal(await normal.evaluate(async () => Object.hasOwn(await import('/src/generated/duel/duel.js'), 'classicMingdianFixture')), false)
  assert.equal(await button(normal, '明典').count(), 0)
  const native = await normal.evaluate(async () => {
    const mod = await import('/src/generated/duel/duel.js')
    const duel = mod.Duel.classic(317, 0, 'guanyu', 8)
    try { return JSON.parse(duel.view(0)).activePlays ?? [] } finally { duel.free() }
  })
  assert.ok(!native.some((play) => play.skill === 'clanmingdian'))
  await normal.close()

  const page = await start(true)
  const board = page.locator('[data-battle-table]')
  const initial = await view(page)
  const play = initial.activePlays.find((item) => item.skill === 'clanmingdian')
  assert.ok(play, 'the real registered skill is offered at the ordinary play phase')
  assert.equal(play.branch.label, '明典')
  assert.equal(play.branch.minCards, 1)
  assert.ok(play.branch.maxCards >= 2)
  const hand = initial.seats[0].hand
  // Both cards are ordinary physical Sha in this existing eight-seat seed.
  const materials = [28, 11]
  for (const id of materials) {
    assert.equal(hand.find((item) => item.id === id)?.kind, 'sha')
    assert.ok(play.branch.cards.includes(id))
  }
  assert.ok(!initial.actions.some((action) => action.type === 'recast' && materials.includes(action.card)),
    'Mingdian does not grant ordinary direct recast')

  await button(page, '明典').click()
  assert.ok(await button(page, '确认').isDisabled())
  for (const held of hand.filter((item) => !['sha', 'shan', 'tao'].includes(item.kind))) {
    assert.ok(!play.branch.cards.includes(held.id), 'non-basic cards are not eligible')
    assert.ok(await card(page, held.id).isDisabled())
  }
  for (const id of materials) {
    assert.ok(await card(page, id).isEnabled())
    await card(page, id).click()
    assert.equal(await card(page, id).getAttribute('aria-pressed'), 'true')
  }
  assert.ok(await button(page, '确认').isEnabled())
  await shot(page, 'selected')
  await page.setViewportSize({ width: 844, height: 390 })
  for (const locator of [card(page, materials[0]), card(page, materials[1]), button(page, '确认')]) {
    const box = await locator.boundingBox()
    assert.ok(box && box.x >= 0 && box.y >= 0 && box.x + box.width <= 844 && box.y + box.height <= 390)
  }
  await shot(page, 'landscape')
  await page.setViewportSize({ width: 1280, height: 720 })
  await button(page, '确认').click()

  const after = await view(page)
  assert.equal(after.version, initial.version + 1)
  assert.equal(Number(await board.getAttribute('data-revision')), after.version)
  assert.equal(after.deckCount, initial.deckCount - materials.length)
  assert.equal(after.seats[0].handCount, initial.seats[0].handCount)
  for (const id of materials) {
    assert.ok(after.discard.some((item) => item.id === id))
    assert.ok(!after.seats[0].hand.some((item) => item.id === id))
    assert.equal(await card(page, id).count(), 0)
  }
  const replacement = after.seats[0].hand.filter((item) => !hand.some((old) => old.id === item.id))
  assert.equal(replacement.length, materials.length)
  const root = recastEvents(after.events, 0, materials.length)
  const revealed = after.events.filter((event) => event.type === 'recastReveal' && event.status === 'started' && event.seat === 0)
  assert.deepEqual(revealed.map((event) => event.card.id), materials)
  for (let id = 1; id < 8; id++) {
    const observer = await view(page, id)
    assert.equal(observer.seats[0].hand, null)
    for (const drawn of replacement) {
      assert.ok(!(observer.seats[0].knownHand ?? []).some((item) => item.id === drawn.id))
      assert.ok(!observer.cardFaces.some((face) => face.id === drawn.id))
    }
  }
  await page.locator('.classic-log summary').click()
  const logs = await page.locator('.classic-log li').allTextContents()
  assert.deepEqual(logs.filter((line) => line.includes('关羽重铸')), ['关羽重铸【杀】、【杀】'])
  await shot(page, 'resolved')
  await page.locator('.classic-log summary').click()

  // Finish the human turn using the table UI and let the next real bot decide.
  await page.clock.runFor(1800)
  await button(page, '结束出牌').click()
  let state = await view(page)
  if (state.waitingFor === 0 && state.choice?.kind === 'discard') {
    await page.clock.runFor(1000)
    for (const id of state.choice.cards.slice(0, state.choice.min)) await card(page, id).click()
    await button(page, '确认').click()
    state = await view(page)
  }
  assert.equal(state.waitingFor, 1, 'the next actual seat is controlled by AI')
  const botStart = state.version
  let botRecast = false
  for (let i = 0; i < 180; i++) {
    await page.clock.runFor(250)
    state = await view(page)
    const botRoots = state.events.filter((event) => event.type === 'recast' && event.status === 'finished' && event.seat === 1)
    if (botRoots.length) {
      const count = state.events.filter((event) => event.type === 'recastReveal' && event.status === 'started' && event.seat === 1).length
      assert.ok(count >= 1)
      recastEvents(state.events, 1, count)
      botRecast = true
      break
    }
    if (state.waitingFor === 0 && state.turn?.seat === 1 && state.choice) {
      const choice = state.choice
      if (choice.counter && choice.branches.length === 1 && choice.branches[0].id === 0) continue
      await page.clock.runFor(1400)
      if (choice.kind === 'shan' || choice.kind === 'save') {
        await button(page, '放弃').click()
      } else {
        const pass = choice.branches.find((branch) => branch.id === 0)
        assert.ok(pass, 'a response during the AI turn can be declined')
        await button(page, pass.label).click()
        await button(page, '确认').click()
      }
    }
    if (state.turn?.seat !== 1 || state.winner || state.drawn) break
  }
  assert.ok(state.version > botStart, 'the AI accepted at least one natural decision')
  assert.ok(botRecast, 'the next AI seat used its own registered Mingdian')
  assert.equal(await page.locator('.game-error').count(), 0)
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, materials, recast: root.id, drawn: replacement.map((item) => item.id), botRecast, botVersion: state.version, errors, favicon404: [...favicon404], evidence }))
} finally {
  await browser.close()
}
