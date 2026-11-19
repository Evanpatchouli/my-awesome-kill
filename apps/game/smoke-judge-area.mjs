import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const { chromium } = createRequire(import.meta.url)(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const bindings = process.env.MAK_JUDGE_AREA_BINDINGS
assert.ok(bindings, 'Set MAK_JUDGE_AREA_BINDINGS to the qa-judge-area wasm-bindgen output')
const js = await fs.readFile(path.join(bindings, 'duel.js'), 'utf8')
const evidence = process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence) {
  const root = path.resolve(fileURLToPath(new URL('../../', import.meta.url)))
  assert.ok(evidence !== root && !evidence.startsWith(root + path.sep))
  await fs.mkdir(evidence, { recursive: true })
}
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const errors = []
const page = await browser.newPage({ locale: 'zh-CN', viewport: { width: 1280, height: 720 } })
const button = name => page.getByRole('button', { name, exact: true })
const target = id => page.locator(`[data-seat="${id}"] .seat-select`)
const view = () => page.evaluate(() => JSON.parse(window.judgeTable.view(0)))

async function submit(label, id, card) {
  await button(label).click()
  if (card != null) await page.locator(`[data-hand] [data-card-id="${card}"]`).click()
  await target(id).click()
  await button('确认').click()
  await page.clock.runFor(1000)
}

async function finishDiscard(label) {
  assert.deepEqual((await view()).choice.branches.map(b => b.label), ['允许弃置', '取消弃置'])
  await button(label).click()
  await button('确认').click()
  await page.clock.runFor(1000)
  return view()
}

try {
  page.on('pageerror', e => errors.push(e.message))
  page.on('console', m => {
    if (['warning', 'error'].includes(m.type()) && !m.location().url.endsWith('/favicon.ico')) errors.push(m.text())
  })
  await page.route('**/src/generated/duel/duel.js*', route => route.fulfill({
    contentType: 'text/javascript', body: js + '\nDuel.classic = (...args) => (window.judgeTable = classicJudgeAreaFixture(...args))',
  }))
  await page.route('**/src/generated/duel/duel_bg.wasm*', route => route.fulfill({
    contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm'),
  }))
  await page.clock.install({ time: new Date('2026-01-01T12:00:00Z') })
  await page.clock.pauseAt(new Date('2026-01-01T12:01:00Z'))
  await page.addInitScript(() => {
    Math.random = () => 317.1 / 0x7fffffff
    localStorage.setItem('mak-reduced-motion', 'true')
  })
  await page.goto(process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420')
  await button('开始游玩').click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  await button('大乔').click()
  await page.locator('select').selectOption('8')
  await button('开始对局').click()
  await page.waitForURL('**/battle')
  await page.clock.runFor(1000)
  assert.equal(await page.locator('[data-seat]').count(), 8)
  assert.equal(await page.locator('vite-error-overlay').count(), 0)
  const id = 1
  const area = page.locator(`[data-judge-area="${id}"]`)
  await submit('国色', id, 28)
  await page.clock.runFor(3000)
  let state = await view()
  assert.deepEqual(state.seats[id].judges.map(c => c.id), [28])
  assert.deepEqual(state.seats[id].judgeAssignments.map(c => c.kind), ['lebu'])
  const hand = state.seats[id].handCount
  await submit('废除判定区', id)
  state = await view()
  assert.ok(state.seats[id].judgeDisabled && !state.seats[id].judgeDisabledMarker)
  assert.ok(!(await area.innerText()).includes('已废除'))
  state = await finishDiscard('取消弃置')
  assert.ok(state.seats[id].judgeDisabledMarker)
  assert.deepEqual(state.seats[id].judges.map(c => c.id), [28])
  assert.match(await area.innerText(), /已废除/)
  assert.match(await area.locator('.retained-judge').innerText(), /乐不思蜀/)
  await page.setViewportSize({ width: 640, height: 272 })
  assert.ok(await area.evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'the retained card and the closed area fit the seat')
  if (evidence) await page.screenshot({ path: path.join(evidence, 'judgment-kept.png') })
  await submit('废除判定区', id)
  state = await finishDiscard('允许弃置')
  assert.equal(state.seats[id].judges.length, 0)
  assert.equal(state.seats[id].judgeAssignments.length, 0)
  assert.equal(state.seats[id].handCount, hand)
  assert.equal(state.discard.filter(c => c.id === 28).length, 1)
  assert.ok(state.virtualPlays.filter(p => p.kind === 'lebu').every(p => !p.branch.targets.includes(id)))
  assert.match(await area.innerText(), /已废除/)
  await submit('恢复判定区', id)
  state = await view()
  assert.ok(!state.seats[id].judgeDisabled && !state.seats[id].judgeDisabledMarker)
  assert.ok(state.virtualPlays.some(p => p.kind === 'lebu' && p.branch.targets.includes(id)))
  assert.ok(!(await area.innerText()).includes('已废除'))
  await submit('国色', id, 40)
  await page.clock.runFor(3000)
  state = await view()
  assert.deepEqual(state.seats[id].judges.map(c => c.id), [40])
  assert.equal(state.discard.filter(c => c.id === 28).length, 1)
  if (evidence) await page.screenshot({ path: path.join(evidence, 'judgment-restored.png') })
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ seed: 317, target: id, discarded: 28, newMaterial: 40, evidence }))
} finally {
  await browser.close()
}
