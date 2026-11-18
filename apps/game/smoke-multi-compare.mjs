// Run against Vite with a separate duel-web build using the non-default qa-multi-compare feature.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_MULTI_BINDINGS
assert.ok(bindings, 'Set MAK_MULTI_BINDINGS to the qa-multi-compare wasm-bindgen output')
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
const view = (page, id = 0) => page.evaluate((viewer) => JSON.parse(window.multiDuel.view(viewer)), id)
const shot = (page, name) => evidence && page.screenshot({ path: path.join(evidence, `multi-${name}.png`) })

async function start(fixture, seed = 317, scenario = "gushe") {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type())) errors.push(`${message.location().url} ${message.text()}`)
  })
  if (fixture) {
    const factory = `Duel.classic = (...args) => (window.multiDuel = classicMultiCompareFixture(...args, "${scenario}"))`
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
  await button(page, '张飞').click()
  await page.locator('select').selectOption('8')
  await button(page, '开始对局').click()
  await page.waitForURL('**/battle')
  assert.equal(await page.locator('[data-seat]').count(), 8)
  assert.equal(await page.locator('vite-error-overlay').count(), 0)
  await page.clock.runFor(1000)
  return page
}

const projections = (page) => page.evaluate(() => Array.from({ length: 9 }, (_, id) => JSON.parse(id < 8 ? window.multiDuel.view(id) : window.multiDuel.view())))
const bids = (page) => page.locator('[data-compare-bid]').evaluateAll((nodes) => nodes.map((node) => Number(node.dataset.compareBid)))
async function continueChoice(page, label) {
  await page.clock.runFor(1200)
  await button(page, label).click(); await button(page, '确认').click()
  await page.clock.runFor(1000)
}
async function showPanel(page) {
  const toggle = button(page, '拼点牌')
  if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click()
}
async function inspectPrivate(page, chooser) {
  const views = await projections(page)
  assert.equal(views[chooser].choice.seat, chooser)
  assert.deepEqual(views[chooser].choice.compareGroup, { initiator: 0, targets: [1, 3, 4] })
  for (const [viewer, state] of views.entries()) {
    if (viewer !== chooser) assert.equal(state.choice, null)
    assert.deepEqual(state.groupUpdates ?? [], [])
    assert.deepEqual(state.privateChoiceCards ?? [], [])
    for (const [owner, id] of [[0,104], [1,105], [3,106], [4,107]]) {
      if (viewer !== owner) assert.ok(!state.cardFaces.some((face) => face.id === id))
      assert.ok(!state.resolving.some((card) => card.id === id))
    }
  }
  assert.equal(await page.locator('[data-compare-bid]').count(), 0)
}
try {
  const normal = await start(false)
  assert.equal(await normal.evaluate(async () => Object.hasOwn(await import('/src/generated/duel/duel.js'), 'classicMultiCompareFixture')), false)
  assert.equal(await button(normal, '鼓舌').count(), 0)
  assert.equal(await button(normal, '胆烈').count(), 0)
  await normal.close()
  const results = []
  for (const scenario of ['gushe', 'danlie']) {
    const page = await start(true, 317, scenario)
    const initial = await view(page)
    assert.equal(await page.locator('.classic-opponents [data-seat]').count(), 7)
    await button(page, scenario === 'gushe' ? '鼓舌' : '胆烈').click()
    for (const id of [4, 1, 3]) await seat(page, id).locator('.seat-select').click()
    await button(page, '确认').click(); await page.clock.runFor(1600)
    await inspectPrivate(page, 0)
    assert.match(await page.locator('.classic-center h1').innerText(), /甘宁.*黄月英.*吕蒙.*暗选/)
    await shot(page, `${scenario}-private`)
    await card(page, 104).click(); await button(page, '确认').click()
    const seen = new Set()
    for (let i = 0; i < 80; i++) {
      const current = await view(page)
      if (current.waitingFor !== 0 && [1,3,4].includes(current.waitingFor)) {
        await inspectPrivate(page, current.waitingFor)
        seen.add(current.waitingFor)
      }
      if (current.waitingFor === 0 && current.choice?.branches[0]?.label === '继续拼点结算') break
      await page.clock.runFor(150)
    }
    assert.deepEqual([...seen], [1,3,4])
    let current = await view(page)
    assert.equal(current.choice.branches[0].label, '继续拼点结算')
    await showPanel(page)
    if (scenario === 'gushe') {
      assert.deepEqual(await bids(page), [104,105])
      assert.deepEqual(current.groupUpdates[0].bids.map((b) => b.seat), [0,1])
      const spectator = (await projections(page))[8]
      assert.ok(!spectator.cardFaces.some((face) => [106,107].includes(face.id)))
      assert.equal(await page.locator('#compare-panel figcaption b').count(), 0)
      await shot(page, 'pair-first')
      await continueChoice(page, '继续拼点结算'); current = await view(page)
      assert.deepEqual(await bids(page), [104,105,104,106])
      assert.deepEqual(current.groupUpdates.find((u) => u.type === 'pair').numbers, [10,8])
      assert.equal(await page.locator('#compare-panel section').count(), 2)
      assert.ok(!(await page.locator('#compare-panel').innerText()).includes('吕蒙'))
      assert.ok(!(await projections(page))[8].cardFaces.some((face) => face.id === 107))
      await continueChoice(page, '继续拼点结算'); current = await view(page)
      assert.deepEqual(current.groupUpdates.find((u) => u.type === 'pair').numbers, [10,10])
      assert.equal(current.choice.branches[0].label, '不弃置，令鼓舌者摸牌')
      await continueChoice(page, '不弃置，令鼓舌者摸牌'); current = await view(page)
      assert.deepEqual(await bids(page), [104,105,104,106,104,107])
      await continueChoice(page, '继续拼点结算'); current = await view(page)
      assert.deepEqual(current.groupUpdates.find((u) => u.type === 'pair').numbers, [10,12])
      await continueChoice(page, '不弃置，令鼓舌者摸牌'); current = await view(page)
      const result = current.groupUpdates.find((u) => u.type === 'result')
      assert.deepEqual(result.numbers, [[10,8],[10,10],[10,12]])
      assert.equal((current.seats[0].marks ?? []).find((mark) => mark.skill === 'gushe').count, 2)
      await showPanel(page)
      assert.deepEqual(await page.locator('#compare-panel h2').allInnerTexts(), ['张飞拼点获胜','拼点平局','吕蒙拼点获胜'])
    } else {
      assert.deepEqual(await bids(page), [104,105,106,107])
      assert.equal(await page.locator('#compare-panel figcaption b').count(), 0)
      const reveal = current.groupUpdates.find((update) => update.type === 'reveal')
      assert.deepEqual(reveal.bids.map((b) => b.seat), [0,1,3,4])
      await page.setViewportSize({ width: 844, height: 390 })
      const boxes = await page.locator('[data-compare-bid]').evaluateAll((nodes) => nodes.map((node) => { const r=node.getBoundingClientRect(); return [r.left,r.top,r.right,r.bottom] }))
      assert.ok(boxes.every(([left,top,right,bottom]) => left >= 0 && top >= 0 && right <= 844 && bottom <= 390))
      await shot(page, 'simultaneous-landscape')
      for (let i = 0; i < 4 && current.choice; i++) {
        await continueChoice(page, '继续拼点结算'); current = await view(page)
      }
      const result = current.groupUpdates.find((u) => u.type === 'result')
      assert.deepEqual(result.numbers, [[11,9],[11,10],[11,8]])
      assert.equal(result.winner, 0)
      for (const id of [1,3,4]) assert.equal(current.seats[id].hp, initial.seats[id].hp - 1)
      await showPanel(page)
      assert.equal(await page.locator('#compare-panel h2').innerText(), '张飞拼点获胜')
      assert.deepEqual((await page.locator('#compare-panel figcaption b').allInnerTexts()).map((text) => text.trim()), ['· 11点','· 9点','· 10点','· 8点'])
      // A score modifier on a later opponent can give the initiator different submitted scores.
      const varied = await page.evaluate(async ({ current, reveal, result }) => {
        const { updateGroupComparison } = await import('/src/classicProtocol.ts')
        const shown = updateGroupComparison(current, [], reveal)
        const [row] = updateGroupComparison(current, shown, { ...result, numbers: [[7,9],[13,10],[13,8]] })
        return { first: row.bids[0].points ?? null, rounds: row.rounds.map(({ first, second }) => [first, second]) }
      }, { current, reveal, result })
      assert.equal(varied.first, null)
      assert.deepEqual(varied.rounds, [[7,9],[13,10],[13,8]])
    }
    for (const id of [104,105,106,107]) assert.ok(current.discard.some((c) => c.id === id))
    results.push({ scenario, version: current.version })
    await page.close()
  }
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ results, errors, evidence }))
} finally { await browser.close() }
