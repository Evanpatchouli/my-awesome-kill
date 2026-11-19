import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const file = await readFile(new URL('../../packages/duel-core/data/example-content-pack.json', import.meta.url))

try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  const errors = []
  page.on('pageerror', error => errors.push(error.message))
  await page.clock.install()
  await page.addInitScript(() => {
    localStorage.setItem('mak-reduced-motion', 'true')
    Math.random = () => 7.1 / 0x7fffffff
  })
  await page.route('**/generated/duel/duel.js*', async route => {
    const response = await route.fetch()
    const source = await response.text()
    await route.fulfill({ response, body: `${source}
const createContentTable = RecordedDuel.classicWithContent;
RecordedDuel.classicWithContent = (...args) => (window.contentTable = createContentTable(...args));
` })
  })
  await page.goto((process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420') + '/#/classic')
  const button = name => page.getByRole('button', { name, exact: true })
  const importer = page.getByLabel('导入本地 JSON')
  await importer.setInputFiles({ name: 'broken.json', mimeType: 'application/json', buffer: Buffer.from('invalid') })
  assert.match(await page.getByRole('alert').innerText(), /内容包无效/)
  await importer.setInputFiles({ name: 'example.json', mimeType: 'application/json', buffer: file })
  await button('云游客').click()
  await page.locator('select').selectOption('2')
  await button('开始对局').click()
  const board = page.locator('[data-battle-table]')
  const gallery = board.locator('[data-hand-gallery]')
  const toolbar = board.locator('.classic-hand-toolbar')
  await board.waitFor()
  await page.clock.runFor(1000)
  const view = () => page.evaluate(() => JSON.parse(window.contentTable.view(0)))
  assert.equal((await view()).seats[0].handCount, 7)
  await button('拾遗').click()
  await button('确认').click()
  await page.clock.runFor(1000)
  assert.equal((await view()).seats[0].handCount, 8)
  assert.ok(await button('拾遗').isDisabled())
  await gallery.getByRole('button', { name: /满载而归/ }).first().click()
  assert.match(await board.locator('[data-hand-rule]').innerText(), /摸三张/)
  await board.locator('.self-select').click()
  await button('确认').click()
  await page.clock.runFor(1300)
  assert.match(await board.locator('.classic-center h1').innerText(), /无懈可击.*满载而归/)
  const pass = toolbar.locator('.classic-options button').filter({ hasText: /不使用|放弃/ }).first()
  await pass.click()
  await page.clock.runFor(1600)
  assert.equal((await view()).seats[0].handCount, 10)

  for (let attempt = 0; attempt < 500; attempt++) {
    const state = await view()
    if (state.winner || state.drawn) break
    if (state.waitingFor === 0 && !(await button('结束出牌').count() && await button('结束出牌').isEnabled()) && !state.choice) {
      await page.clock.runFor(500)
      continue
    }
    if (state.waitingFor === 0 && state.choice?.kind === 'discard') {
      const candidates = gallery.locator('[data-card-id]:enabled')
      if (!(await candidates.count())) { await page.clock.runFor(500); continue }
      for (let index = 0; index < state.choice.min; index++) await candidates.nth(index).click()
      await button('确认').click()
    } else if (state.waitingFor === 0 && ['shan', 'save'].includes(state.choice?.kind)) {
      if (await button('放弃').isEnabled()) await button('放弃').click()
    } else if (state.waitingFor === 0 && state.choice?.counter) {
      const decline = toolbar.locator('.classic-options button:enabled').filter({ hasText: /不使用|放弃/ }).first()
      if (await decline.count()) await decline.click()
    } else if (state.waitingFor === 0 && state.choice?.kind === 'content') {
      const first = state.choice.branches[0]
      const branch = toolbar.getByRole('button', { name: first.label, exact: true })
      const random = board.locator('[data-card-choice] .classic-random-hand button:enabled').first()
      if (await branch.count() && await branch.isEnabled()) {
        await branch.click()
      } else if (await random.count()) {
        await random.click()
      }
      if ((await view()).version !== state.version) continue
      for (const id of first.cards.slice(0, first.minCards)) {
        await board.locator(`[data-card-id="${id}"]:enabled`).click()
      }
      for (let index = 0; index < first.minTargets; index++) {
        await toolbar.locator('.classic-choice-targets button').nth(index).click()
      }
      if (await button('确认').count() && await button('确认').isEnabled()) await button('确认').click()
    } else if (state.waitingFor === 0 && await button('结束出牌').count() && await button('结束出牌').isEnabled()) {
      await button('结束出牌').click()
    }
    await page.clock.runFor(650)
  }
  assert.ok((await view()).winner || (await view()).drawn, 'custom table reaches a terminal state')
  const download = page.waitForEvent('download')
  await button('导出内容包录像').click()
  const exported = await download
  const recording = await readFile(await exported.path())
  const log = JSON.parse(recording)
  assert.equal(log.setup.pack.cards[0].effects[0].count, 3)
  await button('返回').click()
  await page.getByLabel('打开内容包录像').setInputFiles({ name: 'replay.json', mimeType: 'application/json', buffer: recording })
  await button('退出回放').waitFor()
  assert.match(await board.locator('.classic-self').innerText(), /云游客/)
  await page.setViewportSize({ width: 640, height: 272 })
  async function assertReplayPromptInView() {
    const prompt = await board.locator('.classic-center h1').boundingBox()
    const dock = await board.locator('.classic-bottom').boundingBox()
    assert.ok(prompt && dock && prompt.y >= 0 && prompt.y + prompt.height <= dock.y,
      'short landscape replay prompt is visible above the dock')
  }
  await assertReplayPromptInView()
  const progress = page.getByRole('slider')
  await progress.fill(String(log.commands.length))
  await progress.dispatchEvent('change')
  assert.ok(await board.getAttribute('data-result'))
  await assertReplayPromptInView()
  assert.deepEqual(errors, [])
  console.log(`Content import, play, export and replay passed (${log.commands.length} commands).`)
} finally {
  await browser.close()
}
