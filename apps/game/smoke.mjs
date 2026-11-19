import { createRequire } from 'node:module'
import assert from 'node:assert/strict'
import path from 'node:path'
import fs from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
const require = createRequire(import.meta.url)
const { chromium } = process.env.MAK_QA_PLAYWRIGHT
  ? require(process.env.MAK_QA_PLAYWRIGHT)
  : require('playwright')
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')
const evidence =
  process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence && (evidence === root || evidence.startsWith(root + path.sep)))
  throw new Error('Screenshots must be outside the repository')
if (evidence) await fs.mkdir(evidence, { recursive: true })
const browser = await chromium.launch(
  process.env.MAK_QA_BROWSER ? { executablePath: process.env.MAK_QA_BROWSER } : {},
)
const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
const errors = []
page.on('pageerror', (e) => errors.push(e.message))
page.on('console', (msg) => {
  if (msg.type() === 'error') errors.push(msg.text())
})
const shot = async (name) => {
  if (evidence) await page.screenshot({ path: path.join(evidence, name + '.png') })
}
try {
  await page.clock.install()
  await page.addInitScript(() => {
    localStorage.setItem('mak-reduced-motion', 'true')
    Math.random = () => 20.1 / 0x7fffffff
  })
  await page.goto(process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420')
  assert.equal(await page.title(), '随心杀')
  await page.getByRole('button', { name: /开始\s*游玩/ }).click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  const roster = page.locator('.pick-grid button')
  assert.equal(await roster.count(), 25)
  const allNames = new Set(
    await roster.evaluateAll((nodes) => nodes.map((node) => node.getAttribute('aria-label'))),
  )
  const filteredNames = new Set()
  for (const faction of ['魏', '蜀', '吴', '群']) {
    await page
      .locator('.pick-roster nav')
      .getByRole('button', { name: faction, exact: true })
      .click()
    for (const name of await roster.evaluateAll((nodes) =>
      nodes.map((node) => node.getAttribute('aria-label')),
    ))
      filteredNames.add(name)
  }
  assert.deepEqual(
    [...filteredNames].sort(),
    [...allNames].sort(),
    'all 25 generals are reachable through faction filters',
  )
  assert.deepEqual(
    await page.locator('select option').evaluateAll((nodes) => nodes.map((node) => node.value)),
    ['2', '3', '4', '5', '6', '7', '8', '9', '10'],
  )
  await page.locator('.pick-roster nav').getByRole('button', { name: '蜀', exact: true }).click()
  assert.ok((await roster.count()) > 0 && (await roster.count()) < 25)
  await page.locator('.pick-grid').getByRole('button', { name: '赵云', exact: true }).click()
  assert.equal(await page.locator('.pick-detail h2').innerText(), '赵云')
  await page.locator('.pick-roster nav').getByRole('button', { name: '全部', exact: true }).click()
  assert.equal(await roster.count(), 25)
  assert.equal(await page.locator('select').inputValue(), '8')
  await shot('roster')
  await page.getByRole('button', { name: '开始对局' }).click()
  const board = page.locator('.classic-board')
  await board.waitFor({ timeout: 15000 })
  assert.equal(await page.locator('[data-seat]').count(), 8)
  assert.ok(
    [6, 7].includes(
      await page.locator('.classic-seat .seat-identity').filter({ hasText: '身份未知' }).count(),
    ),
    'only the lord identity may be public',
  )
  assert.match(await page.locator('.classic-self').innerText(), /赵云/)
  await shot('eight-seats')
  await page.setViewportSize({ width: 844, height: 390 })
  await page.clock.runFor(1000)
  await shot('eight-seats-landscape')
  const dimensions = await page.evaluate(() => {
    const hand = document.querySelector('[data-hand-gallery]').getBoundingClientRect(),
      controls = document.querySelector('.classic-controls').getBoundingClientRect()
    return {
      width: innerWidth,
      scrollWidth: document.body.scrollWidth,
      height: innerHeight,
      handTop: hand.top,
      handBottom: hand.bottom,
      controlBottom: controls.bottom,
    }
  })
  assert.equal(dimensions.scrollWidth, dimensions.width)
  assert.ok(
    dimensions.handTop >= 0 && dimensions.handBottom <= dimensions.height + 1,
    'hand fits landscape',
  )
  assert.ok(dimensions.controlBottom <= dimensions.handTop + 1, 'actions are above hand')
  for (let i = 0; i < 20 && (await board.getAttribute('data-waiting-for')) === '0'; i++) {
    await page.clock.runFor(500)
    const kind = await board.getAttribute('data-choice-kind')
    const confirm = page.locator('.classic-control-row button').filter({ hasText: '确认' })
    if (kind === 'discard') {
      const cards = page.locator('[data-hand-gallery] .playing-card:enabled')
      for (let n = 0; n < (await cards.count()) && (await confirm.isDisabled()); n++)
        await cards.nth(n).click()
      if (await confirm.isEnabled()) await confirm.click()
    } else if (kind === 'shan' || kind === 'save') {
      const pass = page.locator('.classic-control-row button').filter({ hasText: '放弃' })
      if (await pass.isEnabled()) await pass.click()
    } else if (!kind) {
      const end = page.locator('.classic-control-row button').filter({ hasText: '结束出牌' })
      if (await end.isEnabled()) await end.click()
    }
  }
  assert.notEqual(
    await board.getAttribute('data-waiting-for'),
    '0',
    'exit while a computer is deciding',
  )
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.waitForURL('**/classic')
  await page.clock.runFor(2000)
  assert.ok(page.url().endsWith('/classic'), 'leaving cancels scheduled computer actions')
  await page.locator('select').selectOption('2')
  await page.getByRole('button', { name: '开始对局' }).click()
  await board.waitFor()
  assert.equal(await page.locator('[data-seat]').count(), 2)
  await shot('two-seats-landscape')
  await page.setViewportSize({ width: 1280, height: 720 })
  await page.evaluate(() => {
    window.__observedCues = []
    const record = () =>
      document
        .querySelectorAll('.effect-cue')
        .forEach((el) => window.__observedCues.push(el.dataset.effect))
    new MutationObserver(record).observe(document.querySelector('.classic-board'), {
      childList: true,
      subtree: true,
    })
    record()
  })
  await shot('two-seats')
  const equipmentBefore = new Map()
  let replacements = 0,
    turns = 0,
    actions = 0,
    targeted = 0,
    discard = 0,
    response = 0,
    content = 0,
    cue = 0,
    toggles = 0
  for (let step = 0; step < 2500; step++) {
    await page.clock.runFor(1000)
    const zones = await page
      .locator('[data-seat]')
      .evaluateAll((nodes) =>
        nodes.map((node) => ({
          seat: node.dataset.seat,
          text: node.querySelector('.classic-zones > div')?.textContent ?? '',
        })),
      )
    for (const zone of zones) {
      const previous = equipmentBefore.get(zone.seat) ?? ''
      const slots = (text) =>
        Object.fromEntries(
          [...text.matchAll(/(武|防|\+马|−马) ([^·]+)/g)].map((match) => [
            match[1],
            match[2].trim(),
          ]),
        )
      const oldSlots = slots(previous),
        newSlots = slots(zone.text)
      if (Object.keys(oldSlots).some((key) => newSlots[key] && newSlots[key] !== oldSlots[key]))
        replacements++
      equipmentBefore.set(zone.seat, zone.text)
    }
    if (await board.getAttribute('data-result')) break
    const waiting = await board.getAttribute('data-waiting-for')
    if (waiting !== '0') continue
    const kind = await board.getAttribute('data-choice-kind')
    const phase = await board.getAttribute('data-phase')
    const confirm = page.locator('.classic-control-row button').filter({ hasText: '确认' })
    if (kind === 'shan' || kind === 'save') {
      const pass = page.locator('.classic-control-row button').filter({ hasText: '放弃' })
      if (await pass.isDisabled()) continue
      await pass.click()
      response++
      actions++
      continue
    }
    if (kind === 'content') {
      const options = page.locator('.classic-options button:not([disabled]), .classic-random-hand button:not([disabled])')
      if (!(await options.count())) {
        assert.ok(
          await page.locator('.classic-options button, .classic-random-hand button').count(),
          `content choice has no branches: ${await page.locator('.classic-center').innerText()}`,
        )
        continue
      }
      const before = await board.getAttribute('data-revision')
      await options.first().click()
      content++
      if (await board.getAttribute('data-revision') !== before) {
        actions++
        continue
      }
    } else if (kind === 'discard') {
      const cards = page.locator('[data-hand-gallery] .playing-card:enabled')
      if (!(await cards.count())) continue
      assert.equal(await page.locator('[data-card-choice]').count(), 0, 'discard uses the hand gallery')
      assert.equal(await confirm.isDisabled(), true)
      const first = cards.first()
      await first.click()
      await first.click()
      assert.equal(await first.getAttribute('aria-pressed'), 'false')
      toggles++
      await first.click()
      for (let i = 1; i < (await cards.count()) && (await confirm.isDisabled()); i++)
        await cards.nth(i).click()
      assert.equal(await confirm.isEnabled(), true)
      await shot('discard-selected')
      await confirm.click()
      discard++
      actions++
      continue
    } else if (!kind && phase === 'play') {
      const playable = page.locator('[data-hand-gallery] .playing-card:not([disabled])')
      const end = page.locator('.classic-control-row button').filter({ hasText: '结束出牌' })
      if (!(await playable.count()) || turns % 3 === 2) {
        if (await end.isDisabled()) continue
        await end.click()
        turns++
        actions++
        continue
      }
      await playable.first().click()
    } else continue
    const availableCards = page.locator(
      '[data-hand-gallery] .playing-card:not([disabled]), [data-card-choice] .classic-pool button:not([disabled])',
    )
    for (let i = 0; i < (await availableCards.count()) && (await confirm.isDisabled()); i++) {
      if ((await availableCards.nth(i).getAttribute('aria-pressed')) !== 'true')
        await availableCards.nth(i).click()
    }
    for (let n = 0; n < 10 && (await confirm.isDisabled()); n++) {
      const target = page
        .locator(
          '.targetable .seat-select:not([disabled]):not([aria-pressed="true"]), .targetable .self-select:not([disabled]):not([aria-pressed="true"])',
        )
        .first()
      if (!(await target.count())) break
      await target.click()
      targeted++
    }
    if (await confirm.isEnabled()) {
      const before = await confirm.evaluate((el) => ({
        image: getComputedStyle(el).backgroundImage,
        size: getComputedStyle(el).backgroundSize,
      }))
      await confirm.hover()
      const after = await confirm.evaluate((el) => ({
        image: getComputedStyle(el).backgroundImage,
        size: getComputedStyle(el).backgroundSize,
      }))
      assert.deepEqual(after, before)
      await confirm.click()
      actions++
      if (await page.locator('.effect-cue').count()) cue++
    } else {
      const end = page.locator('.classic-control-row button').filter({ hasText: '结束出牌' })
      if (!kind && (await end.isEnabled())) {
        await end.click()
        turns++
        actions++
      } else
        throw new Error(
          `Cannot complete ${kind ?? phase}: ${await page.locator('.classic-controls').innerText()}`,
        )
    }
  }
  assert.ok(
    await board.getAttribute('data-result'),
    `seed 20 unfinished: version ${await board.getAttribute('data-revision')}; ${await page.locator('.classic-center').innerText()}`,
  )
  assert.ok(actions > 0 && targeted > 0, 'played a card with a target')
  assert.ok(response > 0 && content > 0, 'response and content choices exercised')
  assert.ok(discard > 0 && toggles > 0, 'batch discard selection and deselection exercised')
  assert.ok(replacements > 0, 'same-slot equipment replacement appears in the public area')
  const observedCues = await page.evaluate(() => window.__observedCues)
  console.log('observed feedback', [...new Set(observedCues)])
  assert.ok(observedCues.includes('damage'), 'damage feedback rendered')
  await shot('result')
  await page.clock.runFor(2000)
  await page.getByRole('button', { name: '再来一局' }).click()
  await page.clock.runFor(1000)
  assert.equal(await board.getAttribute('data-result'), null)
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.waitForURL('**/classic')
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.getByRole('button', { name: /我的/ }).click()
  assert.ok(await page.locator('.record-list > div').count(), 'finished game saved to history')
  // Seed 20 puts the human lord first: Zhuge Liang's Guanxing exposes a private preview.
  await page.goto((process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420') + '/#/classic')
  await page.getByRole('button', { name: '诸葛亮', exact: true }).click()
  await page.getByRole('button', { name: '开始对局' }).click()
  await board.waitFor()
  await page.clock.runFor(1000)
  await page.locator('.classic-options button').filter({ hasText: '观星' }).click()
  await page.clock.runFor(1000)
  const arrangement = page.locator('[data-move-choice]')
  const moveControls = page.locator('.classic-hand-toolbar [data-move-controls]')
  await arrangement.waitFor()
  assert.equal(await arrangement.locator('button:not(.playing-card), select').count(), 0)
  const topRow = arrangement.locator('[data-row="0"]')
  const bottomRow = arrangement.locator('[data-row="1"]')
  const rowCards = (row) => row.locator('button[data-card-id]')
  const ids = async (row) => rowCards(row).evaluateAll((cards) => cards.map((card) => Number(card.dataset.cardId)))
  const initialPreview = await ids(topRow)
  assert.ok(initialPreview.length >= 2, 'the acting seat sees the private preview')
  assert.equal(await page.locator('[data-resolving-card]').count(), 0)
  const sendToBottom = async () => {
    await rowCards(topRow).first().click()
    await moveControls.getByRole('button', { name: '移动到牌堆底', exact: true }).click()
  }
  await sendToBottom()
  assert.deepEqual(await ids(bottomRow), initialPreview.slice(0, 1))
  await moveControls.getByRole('button', { name: '撤销移动', exact: true }).click()
  assert.deepEqual(await ids(topRow), initialPreview)
  // Undo preserves the selected card, so move it to the bottom once more.
  await moveControls.getByRole('button', { name: '移动到牌堆底', exact: true }).click()
  if (await rowCards(topRow).count() > 1) {
    const before = await ids(topRow)
    await rowCards(topRow).last().click()
    await moveControls.getByRole('combobox', { name: '牌堆顶插入位置' }).selectOption('0')
    await moveControls.getByRole('button', { name: '移动到牌堆顶', exact: true }).click()
    assert.deepEqual(await ids(topRow), [before.at(-1), ...before.slice(0, -1)])
  }
  const expectedDraw = (await ids(topRow)).slice(0, 2)
  assert.ok(expectedDraw.length > 0)
  await shot('private-choice')
  await page.setViewportSize({ width: 640, height: 272 })
  const confirmOrder = moveControls.getByRole('button', { name: '确认', exact: true })
  const confirmBox = await confirmOrder.boundingBox()
  assert.ok(confirmBox && confirmBox.x >= 0 && confirmBox.y >= 0 &&
    confirmBox.x + confirmBox.width <= 640 && confirmBox.y + confirmBox.height <= 272)
  const surface = await page.locator('[data-card-choice]').boundingBox()
  await shot('private-choice-short-landscape')
  for (const row of [topRow, bottomRow]) {
    const first = await rowCards(row).first().boundingBox()
    assert.ok(first && first.y >= 0 && first.y + first.height <= surface.y + surface.height,
      `choice card ${JSON.stringify(first)} must fit inside ${JSON.stringify(surface)}`)
  }
  await confirmOrder.click()
  await page.clock.runFor(1000)
  assert.equal(await arrangement.count(), 0, 'the private ordering completes')
  for (const id of expectedDraw) assert.ok(await page.locator(`[data-hand] [data-card-id="${id}"]`).count())
  await page.setViewportSize({ width: 844, height: 390 })
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.waitForURL('**/classic')
  await page.getByRole('button', { name: '开始对局' }).click()
  await board.waitFor()
  assert.ok(
    await page.locator('.effect-cue[data-effect="draw"]').count(),
    'opening draw is in progress',
  )
  await page.getByRole('button', { name: '返回', exact: true }).click()
  await page.clock.runFor(2000)
  assert.ok(page.url().endsWith('/classic'), 'leaving during opening draw cancels the match')
  await page.locator('select').selectOption('8')
  await page.getByRole('button', { name: '刘备', exact: true }).click()
  await page.getByRole('button', { name: '开始对局' }).click()
  await board.waitFor()
  await page.clock.runFor(1000)
  const revision = Number(await board.getAttribute('data-revision'))
  await page.locator('.classic-own-skills').getByRole('button', { name: '仁德', exact: true }).click()
  const skillConfirm = page.locator('.classic-control-row button').filter({ hasText: '确认' })
  assert.equal(await skillConfirm.isDisabled(), true)
  await page.locator('[data-hand-gallery] .playing-card:enabled').first().click()
  assert.equal(await skillConfirm.isDisabled(), true, 'skill requires a legal target')
  await page.locator('.classic-seat .seat-select:enabled').first().click()
  assert.equal(await skillConfirm.isEnabled(), true)
  await shot('skill-target')
  await skillConfirm.click()
  await page.clock.runFor(1000)
  assert.ok(
    Number(await board.getAttribute('data-revision')) > revision,
    'skill branch accepted with chosen card and target',
  )
  assert.deepEqual(errors, [])
  console.log(
    JSON.stringify({
      dimensions,
      actions,
      targeted,
      discard,
      response,
      content,
      cue,
      toggles,
      replacements,
      observedCues: [...new Set(observedCues)],
      evidence,
    }),
  )
} finally {
  await browser.close()
}
