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
const { slot } = await server.ssrLoadModule('/src/cards.ts')
const seed = 20
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
  // Check the brush image survives hover and focus; shorthand background must not reset it.
  const action = page.locator('.action-row button').first()
  await action.waitFor({ state: 'visible' })
  const beforeHover = await action.evaluate((el) => getComputedStyle(el).backgroundImage)
  await action.hover()
  const afterHover = await action.evaluate((el) => getComputedStyle(el).backgroundImage)
  assert.match(beforeHover, /primary\.webp|secondary\.webp/)
  assert.equal(afterHover, beforeHover, 'hover preserves the brush image')
  await action.focus()
  assert.equal(await action.evaluate((el) => getComputedStyle(el).backgroundImage), beforeHover)
  await screenshot(page, 'hover')
  const eventTypes = new Set(), equipmentSeen = new Set(), hoverTypes = new Set(), previousEquipment = [{}, {}]
  let replacementChecks = 0
  let discardChecks = 0, eventChecks = 0, zoneChecks = 0,
    clicks = 0,
    responses = 0,
    locked = 0,
    steps = 0,
    rapidChecks = 0
  while (game.winner === null && steps++ < 400) {
    if (game.actor === 1) {
      game.best()
      continue
    }
    const view = game.view()
    if (!await page.locator('.battle-page').isVisible()) { console.error('missing battle', { step: steps, revision: view.revision, url: page.url(), errors, body: (await page.locator('body').innerText()).slice(0, 1000) }) }
    await page.locator('.battle-page').waitFor()
    await page.waitForFunction((revision) => Number(document.querySelector('.battle-page')?.dataset.revision) === revision, view.revision, { timeout: 45000 }).catch(async (e) => { console.error('revision mismatch', { step: steps, expected: view.revision, actual: await page.locator('.battle-page').getAttribute('data-revision'), prompt: await page.locator('.battle-prompt').innerText().catch(() => ''), errors }); throw e })
    for (const [seat, zone] of [[0, '.player-zone'], [1, '.opponent-zone']]) {
      const text = await page.locator(zone).innerText()
      for (const card of [...view.players[seat].equipment, ...view.players[seat].judges]) assert.ok(text.includes(card.name), `seat ${seat} zone must show ${card.name}`)
      const actualEquipment = view.players[seat].equipment
      for (const [oldSlot, oldCard] of Object.entries(previousEquipment[seat])) {
        const replacement = actualEquipment.find((card) => slot(card.kind) === oldSlot)
        if (replacement && replacement.id !== oldCard.id) {
          assert.ok(text.includes(replacement.name), 'new equipment visible')
          if (replacement.name !== oldCard.name) assert.ok(!text.includes(oldCard.name), 'replaced equipment removed')
          replacementChecks++
          await screenshot(page, 'replacement')
        }
      }
      previousEquipment[seat] = Object.fromEntries(actualEquipment.map((card) => [slot(card.kind), card]))
      for (const card of actualEquipment) equipmentSeen.add(`${seat}:${card.id}`)
      zoneChecks++
    }
    if (view.discardCountRequired) {
      const value = (c) => c.kind === 'tao' ? 10 : ['shan', 'wuxie'].includes(c.kind) ? 7 : c.kind === 'sha' ? 4 : 5
      const ids = [...view.players[0].hand].sort((a, b) => value(a) - value(b)).slice(0, view.discardCountRequired).map((c) => c.id)
      const confirm = page.getByRole('button', { name: '确认弃牌' })
      assert.equal(await confirm.isDisabled(), true)
      for (const id of ids) {
        const index = view.players[0].hand.findIndex((c) => c.id === id)
        const card = page.locator('.playing-card').nth(index)
        await card.click()
        assert.equal(await card.getAttribute('aria-pressed'), 'true')
        if (id === ids[0]) {
          await card.click()
          assert.equal(await card.getAttribute('aria-pressed'), 'false')
          assert.equal(await confirm.isDisabled(), true)
          await card.click()
        }
      }
      assert.equal(await confirm.isEnabled(), true)
      await screenshot(page, 'discard-selected')
      await confirm.click()
      assert.equal(game.best(), true)
      discardChecks++
      continue
    }
    const replacementChoice = game.options.find((option) => {
      const card = view.players[0].hand.find((item) => item.id === option.card)
      return card && slot(card.kind) && view.players[0].equipment.some((item) => slot(item.kind) === slot(card.kind))
    })
    const choice = replacementChoice ?? game.options.reduce((a, b) => (a.score >= b.score ? a : b))
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
    const brushType = await btn.evaluate((el) => el.classList.contains('secondary') ? 'secondary' : 'primary')
    if (!hoverTypes.has(brushType)) {
      const normal = await btn.evaluate((el) => ({ image: getComputedStyle(el).backgroundImage, size: getComputedStyle(el).backgroundSize }))
      await btn.hover()
      const hovered = await btn.evaluate((el) => ({ image: getComputedStyle(el).backgroundImage, size: getComputedStyle(el).backgroundSize }))
      assert.match(normal.image, new RegExp(`${brushType}\\.webp`))
      assert.deepEqual(hovered, normal, `${brushType} hover preserves brush proportions`)
      hoverTypes.add(brushType)
    }
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
    assert.equal(game.act(game.revision, game.key(game.options.indexOf(choice))), true)
    if (game.events.length) {
      await page.waitForFunction((revision) => Number(document.querySelector('.battle-page')?.dataset.revision) === revision, game.revision, { timeout: 10000 })
      for (const event of game.events) {
        assert.ok(await page.locator(`[data-effect="${event.type}"]`).count(), `event ${event.type} must render`)
        eventTypes.add(event.type)
        eventChecks++
      }
    }
    clicks++
    if (game.actor === 0 && game.winner === null) {
      const later = page.locator('.action-row button').first()
      if ((await later.count()) && (await later.isDisabled())) locked++
    }
  }
  assert.notEqual(game.winner, null)
  assert.equal(rapidChecks, 1)
  assert.ok(discardChecks > 0, 'must exercise batch discard')
  assert.ok(replacementChecks > 0, 'must observe equipment replacement')
  assert.deepEqual([...hoverTypes].sort(), ['primary', 'secondary'])
  assert.ok(eventTypes.has('play') && eventTypes.has('damage'), 'must render play and damage cues')
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
      discardChecks, eventChecks, zoneChecks, replacementChecks, hoverTypes: [...hoverTypes], eventTypes: [...eventTypes], equipmentSeen: [...equipmentSeen],
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
