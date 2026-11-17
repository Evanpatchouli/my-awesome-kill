import { createRequire } from 'node:module'
import assert from 'node:assert/strict'
import { pathToFileURL, fileURLToPath } from 'node:url'
import path from 'node:path'
import fs from 'node:fs/promises'
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..'),
  req = createRequire(import.meta.url)
const { chromium } = process.env.MAK_QA_PLAYWRIGHT
  ? req(process.env.MAK_QA_PLAYWRIGHT)
  : req('playwright')
const { createServer } = await import(pathToFileURL(req.resolve('vite')).href)
const server = await createServer({
  root: path.join(root, 'packages/rules'),
  configFile: false,
  appType: 'custom',
  server: { middlewareMode: true },
  logLevel: 'silent',
})
const { Game } = await server.ssrLoadModule('/src/engine.ts')
const seed = 19
const game = new Game(seed, 'zhangfei')
const b = await chromium.launch(
  process.env.MAK_QA_BROWSER ? { executablePath: process.env.MAK_QA_BROWSER } : {},
)
const evidence =
  process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence && (evidence === root || evidence.startsWith(root + path.sep)))
  throw new Error('Screenshot directory must be outside the repository')
if (evidence) await fs.mkdir(evidence, { recursive: true })
const screenshot = async (page, name) => {
  if (evidence) await page.screenshot({ path: path.join(evidence, name + '.png') })
}
try {
  const page = await b.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  const errors = []
  page.on('pageerror', (e) => errors.push(e.message))
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(msg.text())
  })
  await page.addInitScript((seed) => {
    Math.random = () => (seed + 0.1) / 0x7fffffff
  }, seed)
  await page.goto(process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420')
  await page.getByRole('heading', { name: '随心杀' }).waitFor()
  assert.equal(await page.title(), '随心杀')
  await page.waitForTimeout(800)
  await screenshot(page, 'home')
  for (const name of ['尽请期待', '我的', '设置']) {
    await page.getByRole('button', { name: new RegExp(name) }).click()
    await page.getByRole('heading', { name, exact: true }).waitFor()
    if (name === '设置') {
      const sw = page.getByRole('switch')
      await sw.click()
      assert.equal(await sw.getAttribute('aria-checked'), 'true')
      await sw.click()
    }
    await page.getByRole('button', { name: '返回', exact: true }).click()
  }
  await page.getByRole('button', { name: /开始\s*游玩/ }).click()
  assert.equal(await page.locator('.mode-feature').count(), 1)
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  await page.getByRole('button', { name: /开始对局/ }).click()
  await page.locator('.playing-card').first().waitFor()
  await page.setViewportSize({ width: 844, height: 390 })
  await page.waitForTimeout(400)
  await screenshot(page, 'landscape')
  const mobile = await page.evaluate(() => ({
    width: innerWidth,
    scrollWidth: document.body.scrollWidth,
    handTop: document.querySelector('.hand').getBoundingClientRect().top,
    handBottom: document.querySelector('.hand').getBoundingClientRect().bottom,
    height: innerHeight,
  }))
  assert.equal(mobile.scrollWidth, mobile.width, 'landscape board must fit its viewport')
  assert.ok(mobile.handTop >= 0 && mobile.handBottom <= mobile.height + 1, 'all hand card heights must fit the landscape board')
  console.log('mobile', mobile)
  await page.setViewportSize({ width: 1280, height: 720 })
  await screenshot(page, 'battle')
  let clicks = 0,
    responses = 0,
    locked = 0,
    steps = 0,
    rapidChecks = 0
  while (game.winner === null && steps++ < 400) {
    if (game.actor === 1) {
      game.best()
      continue
    }
    const choice = game.options.reduce((a, b) => (a.score >= b.score ? a : b))
    const view = game.view()
    await page.getByRole('heading', { name: view.prompt, exact: true }).waitFor({ timeout: 5000 })
    assert.equal(await page.locator('.playing-card').count(), view.players[0].handCount)
    const i = view.players[0].hand.findIndex((c) => c.id === choice.card)
    if (i >= 0) await page.locator('.playing-card').nth(i).click()
    const visibleChoices = view.choices
      .filter(
        (c) =>
          c.card === undefined ||
          !view.players[0].hand.some((card) => card.id === c.card) ||
          (i >= 0 && c.card === choice.card),
      )
      .map((c) => c.label)
      .sort()
    assert.deepEqual(
      (await page.locator('.action-row button').allTextContents()).map((s) => s.trim()).sort(),
      visibleChoices,
    )
    const btn = page.getByRole('button', { name: choice.label, exact: true })
    await btn.waitFor({ state: 'visible' })
    await page.waitForFunction(
      (label) =>
        [...document.querySelectorAll('.action-row button')].some(
          (button) => button.textContent.trim() === label && !button.disabled,
        ),
      choice.label,
    )
    if (!rapidChecks) {
      const second = await btn.evaluate(async (element) => {
        element.click()
        await new Promise((resolve) => setTimeout(resolve, 0))
        const next = document.querySelector('.action-row button')
        if (!next) return null
        const disabled = next.disabled,
          changed = next !== element
        next.click()
        return { disabled, changed, label: next.textContent.trim() }
      })
      assert.ok(second, 'rapid click must target a subsequent decision')
      assert.equal(second.changed, true, 'rapid click must target the next decision')
      assert.equal(second.disabled, true, 'subsequent decision must reject the rapid click')
      rapidChecks++
    } else await btn.click()
    if (game.phase !== '出牌阶段' || view.prompt.includes('杀') || view.prompt.includes('无懈'))
      responses++
    game.best()
    clicks++
    if (game.actor === 0 && game.winner === null) {
      const later = page.locator('.action-row button').first()
      if ((await later.count()) && (await later.isDisabled())) locked++
    }
  }
  assert.notEqual(game.winner, null)
  assert.equal(rapidChecks, 1)
  await page.getByRole('button', { name: /再来一局/ }).waitFor({ timeout: 10000 })
  await screenshot(page, 'finish')
  await page.getByRole('button', { name: /再来一局/ }).click()
  await page.locator('.header-info').filter({ hasText: /第\s*1\s*回合\s*·\s*牌堆\s*94/ }).waitFor()
  const restart = new Game(seed, 'zhangfei')
  while (restart.actor === 0 && restart.winner === null) {
    const view = restart.view(),
      choice = restart.options.reduce((a, b) => (a.score >= b.score ? a : b))
    await page.getByRole('heading', { name: view.prompt, exact: true }).waitFor()
    const i = view.players[0].hand.findIndex((c) => c.id === choice.card)
    if (i >= 0) await page.locator('.playing-card').nth(i).click()
    await page.getByRole('button', { name: choice.label, exact: true }).click()
    assert.equal(restart.best(), true)
  }
  assert.equal(restart.actor, 1)
  await page.getByText('对手正在思考', { exact: false }).waitFor()
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.waitForURL('**/classic')
  assert.ok(page.url().endsWith('/classic'))
  await page.getByRole('button', { name: /开始对局/ }).click()
  await page.locator('.header-info').filter({ hasText: /第\s*1\s*回合\s*·\s*牌堆\s*94/ }).waitFor()
  const initial = new Game(seed, 'zhangfei').view()
  await page.waitForTimeout(650)
  await page.getByRole('heading', { name: initial.prompt, exact: true }).waitFor()
  assert.equal(await page.locator('.playing-card').count(), initial.players[0].handCount)
  for (let i = 0; i < 3; i++) await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.getByRole('button', { name: /我的/ }).click()
  assert.equal(await page.locator('.record-list>div').count(), 1)
  const record = await page.locator('.record-list').innerText()
  console.log(
    JSON.stringify({
      clicks,
      responses,
      locked,
      rapidChecks,
      steps,
      winner: game.winner,
      turn: game.turn,
      record,
      errors,
      evidence,
    }),
  )
  assert.equal(errors.length, 0)
} finally {
  await b.close()
  await server.close()
}
