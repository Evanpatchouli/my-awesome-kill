// Run against Vite with a separate duel-web build using the non-default qa-qiaoshui feature.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_QIAOSHUI_BINDINGS
assert.ok(bindings, 'Set MAK_QIAOSHUI_BINDINGS to the qa-qiaoshui wasm-bindgen output')
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
const view = (page, id = 0) => page.evaluate((viewer) => JSON.parse(window.qiaoshuiDuel.view(viewer)), id)
const shot = (page, name) => evidence && page.screenshot({ path: path.join(evidence, `qiaoshui-${name}.png`) })

async function start(fixture) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type())) errors.push(`${message.location().url} ${message.text()}`)
  })
  if (fixture) {
    const factory = 'Duel.classic = (...args) => (window.qiaoshuiDuel = classicQiaoshuiFixture(...args))'
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

try {
  const normal = await start(false)
  assert.equal(await normal.evaluate(async () => Object.hasOwn(await import('/src/generated/duel/duel.js'), 'classicQiaoshuiFixture')), false)
  assert.equal(await button(normal, '选择拼点对手').count(), 0)
  await normal.close()

  const page = await start(true)
  const initial = await view(page)
  const [bid, sha, duel] = [104, 106, 107]
  assert.deepEqual(initial.choice.branches.find((b) => b.label === '选择拼点对手').targets, [1, 2, 4, 5, 6, 7])
  await button(page, '选择拼点对手').click()
  await seat(page, 1).locator('.seat-select').click()
  await button(page, '确认').click()
  await page.clock.runFor(1800)
  assert.deepEqual((await view(page)).choice.compare, { initiator: 0, target: 1 })
  assert.equal((await view(page, 1)).choice, null)
  await card(page, bid).click()
  await button(page, '确认').click()
  await page.clock.runFor(6500)
  const won = await view(page)
  const result = won.events.find((event) => event.type === 'compareResult' && event.status === 'started').result
  assert.deepEqual(result.numbers, [13, 1])
  assert.equal(result.winner, 0)
  assert.deepEqual(result.cards.map((c) => c.id), [104, 105])
  assert.ok(won.actions.some((action) => action.type === 'playCard' && action.card === sha && action.targets.join() === '1'))
  assert.ok(!won.actions.some((action) => action.type === 'playCard' && action.card === sha && action.targets.includes(3)))
  await card(page, sha).click()
  assert.ok(await seat(page, 3).locator('.seat-select').isDisabled())
  await seat(page, 1).locator('.seat-select').click()
  await button(page, '确认').click()
  await page.clock.runFor(1800)
  const choice = await view(page)
  assert.ok(choice.resolving.some((c) => c.id === sha))
  assert.ok(choice.choice.branches.find((branch) => branch.label === '巧说：增加目标').targets.includes(3))
  for (let id = 1; id < 8; id++) assert.equal((await view(page, id)).choice, null)
  await button(page, '巧说：增加目标').click()
  await seat(page, 3).locator('.seat-select').click()
  await shot(page, 'extra-target')
  await button(page, '确认').click()
  await page.clock.runFor(10000)
  const struck = await view(page)
  assert.equal(struck.seats[1].hp, initial.seats[1].hp - 1)
  assert.equal(struck.seats[3].hp, initial.seats[3].hp - 1)
  assert.equal(Number(await page.locator('[data-battle-table]').getAttribute('data-revision')), struck.version)
  assert.equal(await seat(page, 1).locator('.health').getAttribute('aria-label'), `体力 ${struck.seats[1].hp} / ${struck.seats[1].maxHp}`)
  assert.equal(await seat(page, 3).locator('.health').getAttribute('aria-label'), `体力 ${struck.seats[3].hp} / ${struck.seats[3].maxHp}`)
  await card(page, duel).click()
  await seat(page, 1).locator('.seat-select').click()
  await button(page, '确认').click()
  await page.clock.runFor(15000)
  const finished = await view(page)
  assert.equal(finished.seats[1].hp, struck.seats[1].hp - 1)
  assert.equal(finished.seats[3].hp, struck.seats[3].hp)
  assert.equal(finished.choice, null)
  assert.equal(await button(page, '巧说：增加目标').count(), 0)
  assert.ok(finished.discard.some((c) => c.id === sha) && finished.discard.some((c) => c.id === duel))
  await shot(page, 'resolved')
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, bids: [104, 105], sha, duel, hp: [1, 3].map((id) => finished.seats[id].hp), errors, evidence }))
} finally {
  await browser.close()
}
