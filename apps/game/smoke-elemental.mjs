// Use Vite dev and web bindings built with duel-web's qa-elemental feature.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_ELEMENTAL_BINDINGS
assert.ok(bindings, 'Set MAK_ELEMENTAL_BINDINGS to the qa-elemental wasm-bindgen output')
const url = process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'
const fixtureJs = await fs.readFile(path.join(bindings, 'duel.js'), 'utf8')
const evidence = process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')
  assert.ok(
    evidence !== root && !evidence.startsWith(root + path.sep),
    'Save evidence outside the repo',
  )
  await fs.mkdir(evidence, { recursive: true })
}
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const errors = []
const button = (page, name) => page.getByRole('button', { name, exact: true })
const seat = (page, id) => page.locator(`[data-seat="${id}"]`)
const card = (page, id) => page.locator(`[data-hand] [data-card-id="${id}"]`)
const view = (page, viewer = 0) =>
  page.evaluate((id) => JSON.parse(window.elementalDuel.view(id)), viewer)
const shot = (page, name) =>
  evidence && page.screenshot({ path: path.join(evidence, `elemental-${name}.png`) })

async function start(useFixture) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type()))
      errors.push(`${message.location().url} ${message.text()}`)
  })
  if (useFixture) {
    const factory =
      'Duel.classic = (...args) => (window.elementalDuel = classicElementalFixture(...args))'
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
    Math.random = () => 317.1 / 0x7fffffff
    localStorage.setItem('mak-reduced-motion', 'true')
  })
  await page.goto(new URL('/#/classic', url).href)
  await button(page, '张飞').click()
  await page.locator('select').selectOption('8')
  await button(page, '开始对局').click()
  await page.waitForURL('**/battle')
  assert.equal(await page.title(), '随心杀')
  assert.equal(await page.locator('[data-seat]').count(), 8)
  assert.equal(await page.locator('vite-error-overlay').count(), 0)
  await page.clock.runFor(1000)
  return page
}

async function settle(page, oldVersion) {
  // The normal store handles each seat's counter and response windows on its own clock.
  const end = button(page, '结束出牌')
  for (let i = 0; i < 30; i++) {
    await page.clock.runFor(400)
    if ((await end.count()) && (await end.isEnabled())) {
      const state = await view(page)
      assert.ok(state.version > oldVersion)
      const revision = await page.locator('[data-battle-table]').getAttribute('data-revision')
      assert.equal(state.version, Number(revision))
      return state
    }
  }
  throw new Error(`Table did not return to play: ${await page.locator('.classic-center h1').innerText()}`)
}

try {
  const normal = await start(false)
  const exposed = await normal.evaluate(async () =>
    Object.hasOwn(await import('/src/generated/duel/duel.js'), 'classicElementalFixture'),
  )
  assert.equal(exposed, false, 'ordinary bindings do not expose the fixture')
  assert.equal(await normal.locator('[data-hand] [data-nature], .classic-seat-state').count(), 0)
  assert.equal(await normal.getByRole('button', { name: /^铁索连环 / }).count(), 0)
  await normal.close()

  const page = await start(true)
  const initial = await view(page)
  const hand = initial.seats[0].hand
  const tiesuo = hand.find((c) => c.kind === 'tiesuo').id
  const fire = hand.find((c) => initial.cardNatures[c.id] === 'fire').id
  const thunder = hand.find((c) => initial.cardNatures[c.id] === 'thunder').id
  const created = [tiesuo, fire, thunder]
  assert.ok(created.every((id) => id >= 104))
  const fallback = await page.evaluate(async () => {
    const { faceOf } = await import('/src/classicProtocol.ts')
    const current = JSON.parse(window.elementalDuel.view(0))
    const standard = current.seats[0].hand.find((c) => c.id < 104)
    return [faceOf(current, standard), faceOf({ ...current, cardFaces: undefined }, standard)]
  })
  assert.deepEqual(fallback[0], fallback[1], 'older standard views retain the same printed face')
  assert.deepEqual((await view(page, 1)).cardFaces.filter((face) => created.includes(face.id)), [])
  assert.deepEqual((await view(page, 1)).cardNatures ?? {}, {})
  for (const [id, name, rank, art] of [
    [tiesuo, '铁索连环', '10♣', 'tiesuo'],
    [fire, '火杀', '4♥', 'sha'],
    [thunder, '雷杀', '5♠', 'sha'],
  ]) {
    const held = card(page, id)
    assert.equal(await held.locator('strong').innerText(), name)
    assert.equal(await held.locator('.card-rank').innerText(), rank)
    const imageLoaded = await held.locator('img').evaluate((img, expected) =>
      img.complete && img.naturalWidth > 0 && img.src.endsWith(`/card/${expected}.png`), art)
    assert.ok(imageLoaded, `original artwork for ${name} loaded`)
  }
  assert.equal(await seat(page, 1).locator('.classic-seat-state').innerText(), '护甲 1')
  assert.equal(await seat(page, 2).locator('.classic-seat-state').innerText(), '已翻面')
  await shot(page, 'opening')

  await card(page, tiesuo).click()
  assert.ok(await button(page, '重铸').isEnabled())
  await seat(page, 0).locator('.self-select').click()
  assert.equal(await seat(page, 0).locator('.target-number').innerText(), '1')
  await seat(page, 0).locator('.self-select').click()
  // Click in reverse seating order; resolve the canonical target list offered by Rust.
  await seat(page, 2).locator('.seat-select').click()
  assert.match(await page.locator('.classic-center h1').innerText(), /继续选择目标/)
  await seat(page, 1).locator('.seat-select').click()
  await seat(page, 1).locator('.seat-select').click()
  assert.equal(await seat(page, 2).locator('.seat-select').getAttribute('aria-pressed'), 'true')
  await seat(page, 1).locator('.seat-select').click()
  assert.equal(await seat(page, 1).locator('.target-number').innerText(), '1')
  assert.equal(await seat(page, 2).locator('.target-number').innerText(), '2')
  await shot(page, 'targets')
  await button(page, '确认').click()
  const linked = await settle(page, initial.version)
  assert.equal(await seat(page, 1).locator('.classic-seat-state').innerText(), '横置 · 护甲 1')
  assert.equal(await seat(page, 2).locator('.classic-seat-state').innerText(), '横置 · 已翻面')
  await shot(page, 'linked')

  await card(page, fire).click()
  await seat(page, 1).locator('.seat-select').click()
  await button(page, '确认').click()
  const burned = await settle(page, linked.version)
  assert.equal(burned.seats[1].hp, initial.seats[1].hp, 'armor absorbs the direct fire hit')
  assert.equal(burned.seats[2].hp, initial.seats[2].hp - 1, 'linked fire reaches the second seat')
  assert.equal(await seat(page, 1).locator('.classic-seat-state').count(), 0)
  assert.equal(await seat(page, 2).locator('.classic-seat-state').innerText(), '已翻面')
  assert.equal(await seat(page, 2).locator('.health').getAttribute('aria-label'), '体力 2 / 3')
  const observer = await view(page, 1)
  assert.equal(observer.cardNatures[fire], 'fire')
  assert.equal(observer.cardNatures[thunder], undefined, 'an unplayed private Sha stays hidden')

  await card(page, thunder).click()
  await seat(page, 1).locator('.seat-select').click()
  await button(page, '确认').click()
  const struck = await settle(page, burned.version)
  assert.equal(struck.seats[1].hp, initial.seats[1].hp - 1)
  assert.equal(struck.seats[2].hp, burned.seats[2].hp, 'damage no longer spreads after unlink')
  assert.equal((await view(page, 1)).cardNatures[thunder], 'thunder')
  await page.setViewportSize({ width: 844, height: 390 })
  await page.locator('.classic-log summary').click()
  const log = await page.locator('.classic-log ol').innerText()
  const expectedLog = [
    '张飞使用【火杀】', '张飞使用【雷杀】', '甘宁的护甲减少1点',
    '甘宁受到1点火焰伤害', '周瑜受到1点火焰伤害', '甘宁受到1点雷电伤害',
  ]
  for (const line of expectedLog) assert.ok(log.includes(line), line)
  await shot(page, 'damage-log')
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({
    seed: 317, created, versions: [initial, linked, burned, struck].map((state) => state.version),
    hp: struck.seats.slice(1, 3).map((player) => player.hp), errors, evidence,
  }))
} finally {
  await browser.close()
}
