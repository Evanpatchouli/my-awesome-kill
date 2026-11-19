// Run against Vite with duel-web built using the qa-xunxun feature.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_XUNXUN_BINDINGS
assert.ok(bindings, 'Set MAK_XUNXUN_BINDINGS to the qa-xunxun wasm-bindgen output')
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
const view = (page, seat = 0) => page.evaluate((id) => JSON.parse(window.xunxunDuel.view(id)), seat)
const shot = (page, name) => evidence && page.screenshot({ path: path.join(evidence, `xunxun-${name}.png`) })
const card = (page, id) => page.locator(`[data-move-choice] [data-card-id="${id}"]`)

try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type()) && !message.location().url.endsWith('/favicon.ico')) errors.push(message.text())
  })
  await page.route('**/src/generated/duel/duel.js*', (route) => route.fulfill({ contentType: 'text/javascript', body: `${fixtureJs}\nDuel.classic = (...args) => (window.xunxunDuel = classicXunxunFixture(...args))` }))
  await page.route('**/src/generated/duel/duel_bg.wasm*', (route) => route.fulfill({ contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm') }))
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
  assert.equal(await page.locator('[data-seat]').count(), 8)
  await page.clock.runFor(1000)
  let state = await view(page)
  assert.equal(state.turn.seat, 0)
  assert.ok(state.choice.branches.some((branch) => branch.id === 1 && branch.label === '发动恂恂'))
  await button(page, '发动恂恂').click()
  await page.clock.runFor(1400)
  state = await view(page)
  const request = state.choice.moveCards
  assert.equal(request.prompt, '恂恂：将两张牌置于牌堆底')
  assert.equal(request.allowDecline, false)
  const ids = request.rows[0].cards
  assert.equal(ids.length, 4)
  assert.deepEqual(request.rows.map((row) => row.label), ['牌堆顶', '牌堆底'])
  for (let seat = 1; seat < 8; seat++) {
    const other = await view(page, seat)
    assert.equal(other.choice, null)
    assert.equal(other.privateChoiceCards.length, 0)
    assert.ok(other.cardFaces.every((face) => !ids.includes(face.id)))
  }
  const arrangement = page.locator('[data-move-choice]')
  const controls = page.locator('.classic-hand-toolbar [data-move-controls]')
  const confirm = controls.getByRole('button', { name: '确认', exact: true })
  assert.equal(await arrangement.locator('button:not([data-card-id]), select').count(), 0)
  assert.ok(await confirm.isDisabled())
  const moveBottom = controls.getByRole('button', { name: '移动到牌堆底', exact: true })
  await card(page, ids[3]).click()
  assert.equal(await card(page, ids[3]).getAttribute('aria-pressed'), 'true')
  await moveBottom.click()
  assert.ok(await confirm.isDisabled())
  await card(page, ids[1]).click()
  await moveBottom.click()
  assert.ok(await confirm.isEnabled())
  await card(page, ids[2]).click()
  assert.ok(await moveBottom.isDisabled())
  await controls.getByLabel('牌堆顶插入位置').selectOption('0')
  await controls.getByRole('button', { name: '移动到牌堆顶', exact: true }).click()
  assert.deepEqual(await arrangement.locator('[data-row="0"] [data-card-id]').evaluateAll((nodes) => nodes.map((n) => Number(n.dataset.cardId))), [ids[2], ids[0]])
  assert.deepEqual(await arrangement.locator('[data-row="1"] [data-card-id]').evaluateAll((nodes) => nodes.map((n) => Number(n.dataset.cardId))), [ids[3], ids[1]])
  await shot(page, 'choice')
  await page.setViewportSize({ width: 640, height: 272 })
  const [surface, actions, gallery, visibleCards] = await Promise.all([
    arrangement.locator('..').boundingBox(),
    controls.boundingBox(),
    page.locator('[data-hand-gallery]').boundingBox(),
    arrangement.locator('[data-card-id]').evaluateAll(cards => cards.filter(card => {
      const bounds = card.getBoundingClientRect()
      const surface = card.closest('.classic-center-surface').getBoundingClientRect()
      const hit = document.elementFromPoint(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2)
      return bounds.top >= surface.top && bounds.bottom <= surface.bottom && card.contains(hit)
    }).length),
  ])
  await shot(page, 'landscape')
  assert.equal(visibleCards, ids.length, 'every card fits inside the middle surface and can be clicked')
  assert.ok(surface && actions && gallery && surface.y >= 0)
  assert.ok(surface.y + surface.height <= actions.y + 1, 'the middle surface fits above the move controls')
  assert.ok(actions.y + actions.height <= gallery.y + 1 && gallery.y + gallery.height <= 273, 'controls and gallery fit the short landscape viewport')
  await page.setViewportSize({ width: 1280, height: 720 })
  const beforeHand = state.seats[0].hand.map((c) => c.id)
  await confirm.click()
  await page.clock.runFor(1000)
  state = await view(page)
  const drawn = state.seats[0].hand.filter((c) => !beforeHand.includes(c.id)).map((c) => c.id)
  assert.deepEqual(drawn, [ids[2], ids[0]])
  for (let seat = 1; seat < 8; seat++) {
    const other = await view(page, seat)
    assert.equal(other.seats[0].hand, null)
    assert.ok(other.cardFaces.every((face) => !ids.includes(face.id)))
  }
  await shot(page, 'resolved')
  await button(page, '结束出牌').click()
  for (let i = 0; i < 240; i++) {
    await page.clock.runFor(250)
    state = await view(page)
    if (state.events.some((event) => event.type === 'registeredTrigger' && event.seat === 1 && event.status === 'finished')) break
    if (state.waitingFor === 0 && state.choice) {
      const choice = state.choice
      if (choice.counter && choice.branches.length === 1 && choice.branches[0].id === 0) continue
      await page.clock.runFor(1400)
      if (choice.kind === 'discard') {
        for (const id of choice.cards.slice(0, choice.min)) await page.locator(`[data-hand] [data-card-id="${id}"]`).click()
        await button(page, '确认').click()
      } else if (choice.kind === 'shan' || choice.kind === 'save') await button(page, '放弃').click()
      else {
        const pass = choice.branches.find((branch) => branch.id === 0)
        if (pass) await button(page, pass.label).click()
      }
    }
    if (state.turn?.seat > 1 || state.winner || state.drawn) break
  }
  const completed = state.events.find((event) => event.type === 'registeredTrigger' && event.seat === 1 && event.status === 'finished')
  assert.ok(completed, 'the next AI seat resolved its registered trigger')
  assert.ok(state.events.some((event) => event.type === 'chooseToMove' && event.parent === completed.id && event.status === 'finished'))
  assert.equal(await page.locator('.game-error, vite-error-overlay').count(), 0)
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, cards: ids, drawn, turn: state.turn, evidence }))
} finally { await browser.close() }
