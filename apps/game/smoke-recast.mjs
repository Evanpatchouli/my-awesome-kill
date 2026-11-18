// Point MAK_RECAST_BINDINGS at the wasm-bindgen web output built with --features qa-recast.
// MAK_SMOKE_URL points at the ordinary Vite dev server.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_RECAST_BINDINGS
assert.ok(bindings, 'Set MAK_RECAST_BINDINGS to the QA wasm-bindgen output directory')
const fixtureJs = await fs.readFile(path.join(bindings, 'duel.js'), 'utf8')
const evidence = process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')
  assert.ok(
    evidence !== root && !evidence.startsWith(root + path.sep),
    'Keep screenshots outside the repository',
  )
  await fs.mkdir(evidence, { recursive: true })
}
const url = process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const errors = []
const favicon404 = new Set()

async function openTable(useFixture) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (!['error', 'warning'].includes(message.type())) return
    const source = message.location().url
    if (source === new URL('/favicon.ico', url).href && message.text().includes('404'))
      favicon404.add(source)
    else errors.push(`${source} ${message.text()}`)
  })
  if (useFixture) {
    // The Vue player entry point and Pinia store retain the real Duel instance and methods.
    const factory =
      'Duel.classic = (...args) => (window.recastTestDuel = classicRecastFixture(...args))'
    await page.route('**/src/generated/duel/duel.js*', (route) =>
      route.fulfill({ contentType: 'text/javascript', body: `${fixtureJs}\n${factory}` }),
    )
    await page.route('**/src/generated/duel/duel_bg.wasm*', (route) =>
      route.fulfill({ contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm') }),
    )
  }
  await page.clock.install({ time: new Date('2026-01-01T12:00:00Z') })
  await page.clock.pauseAt(new Date('2026-01-01T12:01:00Z'))
  await page.addInitScript(() => {
    Math.random = () => 3.1 / 0x7fffffff
    localStorage.setItem('mak-reduced-motion', 'true')
  })
  await page.goto(new URL('/#/classic', url).href)
  await page.getByRole('button', { name: '曹操', exact: true }).click()
  await page.locator('select').selectOption('2')
  await page.getByRole('button', { name: '开始对局' }).click()
  await page.waitForURL('**/battle')
  await page.locator('[data-battle-table]').waitFor()
  assert.equal(await page.title(), '随心杀')
  assert.equal(await page.locator('vite-error-overlay').count(), 0)
  assert.equal(await page.locator('[data-hand] button:enabled').count(), 0, 'opening hand is locked')
  await page.clock.runFor(1000)
  return page
}

const button = (page, name) => page.getByRole('button', { name, exact: true })
const handCard = (page, id) => page.locator(`[data-hand] [data-card-id="${id}"]`)
const view = (page, seat = 0) =>
  page.evaluate((id) => JSON.parse(window.recastTestDuel.view(id)), seat)
const submitRecast = (page, version, seat, card) =>
  page.evaluate((command) => window.recastTestDuel.submit(JSON.stringify(command)), {
    version, seat, action: { type: 'recast', card },
  })
const shot = (page, name) =>
  evidence && page.screenshot({ path: path.join(evidence, `recast-${name}.png`) })

try {
  const normal = await openTable(false)
  const fixtureExported = await normal.evaluate(async () =>
    Object.hasOwn(await import('/src/generated/duel/duel.js'), 'classicRecastFixture'),
  )
  assert.equal(fixtureExported, false, 'production bindings exclude the test factory')
  assert.equal(await button(normal, '重铸').count(), 0)
  assert.ok(await normal.getByRole('button', { name: /^闪 / }).first().isDisabled())
  await normal.getByRole('button', { name: /^杀 / }).first().click()
  assert.equal(await button(normal, '重铸').count(), 0, 'ordinary Sha only offers play')
  await shot(normal, 'standard')
  await normal.locator('[data-seat="1"] .seat-select').click()
  await button(normal, '确认').click()
  assert.equal(await normal.locator('[data-battle-table]').getAttribute('data-revision'), '1')
  await normal.close()

  const page = await openTable(true)
  const board = page.locator('[data-battle-table]')
  const recast = button(page, '重铸')
  const confirm = button(page, '确认')
  const initial = await view(page)
  const hand = initial.seats[0].hand
  const shan = hand.find((card) => card.kind === 'shan').id
  const sha = hand.find((card) => card.kind === 'sha').id
  const control = hand.find((card) => card.kind === 'wugu').id
  assert.deepEqual((await view(page, 1)).actions, [], 'other seat has no private actions')
  await assert.rejects(submitRecast(page, initial.version, 1, shan), /WrongActor/)
  assert.equal((await view(page)).version, initial.version)
  assert.equal(await recast.count(), 0)
  await handCard(page, sha).click()
  assert.ok(await recast.isEnabled())
  await handCard(page, control).click()
  assert.equal(await recast.count(), 0, 'selection of an unregistered card clears recast')

  const results = []
  for (const [id, kind] of [[shan, 'only'], [sha, 'dual']]) {
    const before = await view(page)
    assert.ok(before.actions.some((a) => a.type === 'recast' && a.card === id))
    assert.equal(before.actions.some((a) => a.type === 'playCard' && a.card === id), kind === 'dual')
    await handCard(page, id).click()
    assert.equal(await handCard(page, id).getAttribute('aria-pressed'), 'true')
    assert.ok(await recast.isEnabled())
    if (kind === 'only') {
      assert.equal(await confirm.count(), 0)
    } else {
      await page.setViewportSize({ width: 844, height: 390 })
      const endPlayLines = await button(page, '结束出牌').evaluate((element) => {
        const range = document.createRange()
        range.selectNodeContents(element)
        return range.getClientRects().length
      })
      assert.equal(endPlayLines, 1, 'three landscape controls keep their labels on one line')
      assert.ok(await confirm.isDisabled(), 'direct recast requires no play target')
      const target = page.locator('[data-seat="1"] .seat-select')
      await target.click()
      assert.ok(await confirm.isEnabled(), 'normal play remains available with a target')
      await target.click()
      assert.ok(await confirm.isDisabled())
    }
    const box = await recast.boundingBox()
    const viewport = page.viewportSize()
    assert.ok(box && box.x >= 0 && box.x + box.width <= viewport.width)
    assert.ok(box.y >= 0 && box.y + box.height <= viewport.height)
    await shot(page, kind)
    await recast.click()
    const after = await view(page)
    assert.equal(after.version, before.version + 1)
    assert.equal(Number(await board.getAttribute('data-revision')), after.version)
    assert.ok(after.discard.some((card) => card.id === id))
    assert.equal(after.deckCount, before.deckCount - 1)
    assert.equal(after.seats[0].handCount, before.seats[0].handCount)
    const replacement = after.seats[0].hand.find(card => !before.seats[0].hand.some(held => held.id === card.id))
    assert.ok(replacement, 'a replacement reached the acting hand')
    const other = await view(page, 1)
    assert.equal(other.seats[0].hand, null)
    assert.equal(other.seats[0].knownHand?.some(card => card.id === replacement.id) ?? false, id === sha,
      'only the configured face-up gain is revealed to the other seat')
    assert.equal(after.events.filter(event => event.type === 'recastReveal' && event.status === 'started' && event.card?.id === id).length, 1)
    assert.equal(await handCard(page, id).count(), 0)
    assert.equal(await recast.count(), 0, 'acceptance clears the selected action')
    assert.equal(await page.locator('[data-hand] button:enabled').count(), 0, 'settling locks the hand')
    results.push({ card: id, kind, version: after.version, deck: after.deckCount })
    await page.clock.runFor(1000)
  }
  await assert.rejects(submitRecast(page, initial.version, 0, shan), /WrongVersion/)
  assert.equal((await view(page)).version, initial.version + 2)
  const log = page.locator('.classic-log summary')
  await log.click()
  assert.ok(await page.getByText('曹操重铸【闪】', { exact: true }).isVisible())
  assert.ok(await page.getByText('曹操重铸【杀】', { exact: true }).isVisible())
  await shot(page, 'resolved')
  await log.click()

  await button(page, '结束出牌').click()
  await page.clock.runFor(1000)
  const discard = (await view(page)).choice
  assert.equal(discard.kind, 'discard')
  for (const id of discard.cards.slice(0, discard.min)) await handCard(page, id).click()
  assert.equal(await recast.count(), 0, 'discard selection never offers recast')
  await confirm.click()
  assert.equal(await board.getAttribute('data-waiting-for'), '1')
  assert.deepEqual((await view(page)).actions, [])
  assert.equal(await recast.count(), 0, 'inactive human seat has no recast control')
  assert.equal(await page.locator('.classic-hand button:enabled').count(), 0)
  await shot(page, 'other-seat')
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({
    url, results, inactiveSeat: 0, evidence, errors, favicon404: [...favicon404],
  }))
} finally {
  await browser.close()
}
