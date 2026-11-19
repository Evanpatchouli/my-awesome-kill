import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const evidence = process.env.MAK_SMOKE_SCREENSHOTS
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const button = (page, name) => page.getByRole('button', { name, exact: true })
if (evidence) await fs.mkdir(evidence, { recursive: true })

async function begin(page, seed, hero, seats = 8, reducedMotion = true) {
  await page.addInitScript(({ gameSeed, reducedMotion }) => {
    Math.random = () => (gameSeed + 0.1) / 0x7fffffff
    localStorage.setItem('mak-reduced-motion', String(reducedMotion))
  }, { gameSeed: seed, reducedMotion })
  assert.ok((await page.goto(process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'))?.ok())
  assert.equal(await page.title(), '随心杀')
  await page.getByRole('button', { name: /开始\s*游玩/ }).click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  await button(page, hero).click()
  await page.locator('select').selectOption(String(seats))
  await button(page, '开始对局').click()
}

async function assertToolbarPosition(board) {
  const bounds = await board.evaluate((element) => {
    const rect = (selector) => element.querySelector(selector).getBoundingClientRect()
    return {
      centerBottom: rect('.classic-center-surface').bottom,
      toolbarTop: rect('.classic-hand-toolbar').top,
      toolbarBottom: rect('.classic-hand-toolbar').bottom,
      galleryTop: rect('[data-hand-gallery]').top,
      galleryBottom: rect('[data-hand-gallery]').bottom,
      viewportBottom: window.innerHeight,
    }
  })
  assert.ok(bounds.centerBottom <= bounds.toolbarTop + 1, 'toolbar is below center')
  assert.ok(bounds.toolbarBottom <= bounds.galleryTop + 1, 'toolbar is above gallery')
  assert.ok(bounds.galleryBottom <= bounds.viewportBottom + 1, 'gallery is visible without scrolling')
}

async function assertCentered(board) {
  const [table, surface, toolbar] = await Promise.all([
    board.boundingBox(),
    board.locator('.classic-center-surface').boundingBox(),
    board.locator('.classic-hand-toolbar').boundingBox(),
  ])
  const dx = surface.x + surface.width / 2 - (table.x + table.width / 2)
  const dy = (surface.y + toolbar.y + toolbar.height) / 2 - (table.y + table.height / 2)
  assert.ok(Math.abs(dx) <= 1 && Math.abs(dy) <= 2, `action group offset from table center: ${dx}, ${dy}`)
}

async function assertCardSizes(board, cards) {
  const [hand, sizes] = await Promise.all([
    board.locator('[data-hand-gallery] .playing-card').first().boundingBox(),
    cards.evaluateAll((elements) => elements.map((card) => {
      const { width, height } = card.getBoundingClientRect()
      return { width, height }
    })),
  ])
  assert.ok(hand && sizes.length > 0)
  for (const { width, height } of sizes) {
    assert.ok(Math.abs(hand.width - width) <= 0.5 && Math.abs(hand.height - height) <= 0.5,
      `card ${width}×${height} matches hand ${hand.width}×${hand.height}`)
  }
}

async function assertPlayedCard(board, id, name, rank) {
  const surface = board.locator('.classic-center-surface')
  const card = surface.locator(`[data-resolving-card="${id}"]`)
  assert.equal(await card.locator('strong').innerText(), name)
  assert.equal(await card.locator('.card-rank').innerText(), rank)
  await card.locator('.card-art').evaluate(img => img.decode())
  assert.equal(await card.evaluate(element => element.tagName), 'DIV')
  assert.doesNotMatch(await surface.innerText(), /结算区/)
  const [frame, face] = await Promise.all([surface.boundingBox(), card.boundingBox()])
  assert.ok(frame && face && face.y >= frame.y && face.y + face.height <= frame.y + frame.height)
  await assertCardSizes(board, card)
}

try {
  for (const viewport of [{ width: 1280, height: 720 }, { width: 640, height: 272 }]) {
    const page = await browser.newPage({ viewport, locale: 'zh-CN' })
    const errors = []
    page.on('pageerror', error => errors.push(error.message))
    page.on('console', message => {
      if (['error', 'warning'].includes(message.type())) errors.push(message.text())
    })
    try {
      await begin(page, 20, '张飞')
      const board = page.locator('[data-battle-table]')
      const gallery = board.locator('[data-hand-gallery]')
      const dismantle = gallery.getByRole('button', { name: /^过河拆桥 / }).first()
      await dismantle.click()
      await assertCardSizes(board, board.locator('[data-effect="draw"] [data-effect-card]'))
      assert.match(await board.locator('[data-hand-rule]').innerText(), /过河拆桥.*弃置/s)
      if (evidence) await page.screenshot({ path: path.join(evidence, `selected-${viewport.width}.png`) })

      const target = board.locator('.classic-seat .seat-select:enabled').first()
      await target.click()
      await button(page, '确认').click()
      const central = board.locator('[data-card-choice]')
      const hidden = central.locator('.card-back')
      const first = hidden.first()
      await first.waitFor()
      await assertPlayedCard(board, 90, '过河拆桥', '4♠')
      await assertCardSizes(board, hidden)
      assert.ok(await hidden.count() >= 2)
      assert.equal(await hidden.locator('.card-art, .card-rank, strong').count(), 0)
      assert.equal(await gallery.locator('button:enabled').count(), 0)
      const confirm = button(board.locator('.classic-hand-toolbar'), '确认')
      assert.equal(await central.locator('button:not(.playing-card)').count(), 0)
      assert.equal(await confirm.count(), 0)
      await first.click()
      assert.equal(await first.getAttribute('aria-pressed'), 'true')
      const rect = await central.boundingBox()
      assert.ok(rect.y >= 0 && rect.y + rect.height <= viewport.height + 1)
      const confirmBox = await confirm.boundingBox()
      assert.ok(confirmBox.y + confirmBox.height <= viewport.height, 'confirm is visible without scrolling')
      await assertToolbarPosition(board)
      if (viewport.height === 720) await assertCentered(board)
      if (evidence) await page.screenshot({ path: path.join(evidence, `choice-${viewport.width}.png`) })
      const version = Number(await board.getAttribute('data-revision'))
      await confirm.click()
      assert.ok(Number(await board.getAttribute('data-revision')) > version)
      assert.equal(await page.locator('vite-error-overlay, .game-error').count(), 0)
      assert.deepEqual(errors, [])
      console.log(`Central hidden-hand selection passed at ${viewport.width}×${viewport.height}`)
    } finally {
      await page.close()
    }

    for (const response of ['放弃', '使用闪', '龙胆', '万箭齐发']) {
      const page = await browser.newPage({ viewport, locale: 'zh-CN' })
      const errors = []
      page.on('pageerror', error => errors.push(error.message))
      page.on('console', message => {
        if (['error', 'warning'].includes(message.type())) errors.push(message.text())
      })
      try {
        // These two seeds reach a Sha or Wanjian response through normal AI turns.
        const groupAttack = response === '万箭齐发'
        await begin(page, groupAttack ? 766 : 166, '赵云')
        const board = page.locator('[data-battle-table]')
        const toolbar = board.locator('.classic-hand-toolbar')
        const gallery = board.locator('[data-hand-gallery]')
        const choice = button(toolbar, groupAttack ? '使用闪' : response)
        const confirm = button(toolbar, '确认')
        await choice.and(toolbar.locator('button:enabled')).waitFor()
        await assertPlayedCard(board, groupAttack ? 76 : 17, groupAttack ? '万箭齐发' : '杀', groupAttack ? 'A♥' : '10♣')
        if (groupAttack) {
          const heading = board.locator('.classic-center h1')
          const expected = '应对【万箭齐发】，请打出一张【闪】。'
          assert.equal(await heading.innerText(), expected)
          await button(toolbar, '龙胆').click()
          assert.equal(await heading.innerText(), expected)
          await button(toolbar, '取消').click()
        }
        assert.equal(await board.locator('[data-card-choice]').count(), 0)
        assert.equal(await confirm.count(), 0)
        await assertToolbarPosition(board)
        if (viewport.height === 720) await assertCentered(board)
        const before = Number(await board.getAttribute('data-revision'))
        if (response === '龙胆') {
          if (evidence) await page.screenshot({ path: path.join(evidence, `response-${viewport.width}.png`) })
          await choice.click()
          const candidates = gallery.locator('button:enabled')
          assert.equal(await candidates.count(), 2)
          assert.equal(await confirm.isDisabled(), true)
          const card = candidates.first()
          await card.click()
          assert.equal(await card.getAttribute('aria-pressed'), 'true')
          await assertPlayedCard(board, 17, '杀', '10♣')
          await assertToolbarPosition(board)
          if (viewport.height === 720) await assertCentered(board)
          if (evidence) await page.screenshot({ path: path.join(evidence, `longdan-${viewport.width}.png`) })
          const id = await card.getAttribute('data-card-id')
          const material = gallery.locator(`[data-card-id="${id}"]`)
          await button(toolbar, '取消').click()
          assert.equal(await material.getAttribute('aria-pressed'), 'false')
          assert.equal(await confirm.count(), 0)
          assert.equal(Number(await board.getAttribute('data-revision')), before)
          await choice.click()
          await material.click()
          await confirm.click()
          await material.waitFor({ state: 'detached' })
        } else {
          if (groupAttack && evidence) await page.screenshot({ path: path.join(evidence, `wanjian-${viewport.width}.png`) })
          await choice.click()
          const shan = gallery.locator(`[data-card-id="${groupAttack ? 38 : 41}"]`)
          assert.equal(await shan.count(), response === '放弃' ? 1 : 0)
        }
        assert.ok(Number(await board.getAttribute('data-revision')) > before)
        assert.equal(await page.locator('vite-error-overlay, .game-error').count(), 0)
        assert.deepEqual(errors, [])
        console.log(`${response} passed at ${viewport.width}×${viewport.height}`)
      } finally {
        await page.close()
      }
    }
  }

  const animated = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  try {
    await begin(animated, 20, '张飞', 8, false)
    const board = animated.locator('[data-battle-table]')
    await board.locator('[data-hand-gallery]').getByRole('button', { name: /^过河拆桥 / }).first().click()
    await board.locator('.classic-seat .seat-select:enabled').first().click()
    await button(animated, '确认').click()
    const frame = await animated.waitForFunction(() => {
      const card = document.querySelector('[data-effect="play"] [data-effect-card="90"]')
      const hand = document.querySelector('[data-hand-gallery] .playing-card')
      if (!card || !hand || Number(getComputedStyle(card.parentElement).opacity) <= 0.05) return null
      const { width, height } = card.getBoundingClientRect()
      const handBox = hand.getBoundingClientRect()
      return {
        width, height, handWidth: handBox.width, handHeight: handBox.height,
        name: card.querySelector('strong').textContent,
        whiteSpace: getComputedStyle(card.querySelector('strong')).whiteSpace,
      }
    })
    const { width, height, handWidth, handHeight, name, whiteSpace } = await frame.jsonValue()
    assert.equal(name, '过河拆桥')
    assert.equal(whiteSpace, 'normal')
    assert.ok(Math.abs(width - handWidth) <= 0.5 && Math.abs(height - handHeight) <= 0.5)
    console.log('Moving card keeps the hand card dimensions')
  } finally {
    await animated.close()
  }

  const desktop = await browser.newPage({ viewport: { width: 1920, height: 1009 }, locale: 'zh-CN' })
  try {
    await begin(desktop, 634, '张飞')
    const board = desktop.locator('[data-battle-table]')
    const toolbar = board.locator('.classic-hand-toolbar')
    const pass = button(toolbar, '放弃').and(toolbar.locator('button:enabled'))
    await pass.waitFor()
    assert.equal(await toolbar.locator('button').count(), 1)
    await assertPlayedCard(board, 76, '万箭齐发', 'A♥')
    await assertToolbarPosition(board)
    await assertCentered(board)
    if (evidence) await desktop.screenshot({ path: path.join(evidence, 'wanjian-1920.png') })
    const before = Number(await board.getAttribute('data-revision'))
    await pass.click()
    assert.ok(Number(await board.getAttribute('data-revision')) > before)
    console.log('Wanjian with only pass centered at 1920×1009')
  } finally {
    await desktop.close()
  }

  const shortPage = await browser.newPage({ viewport: { width: 640, height: 272 }, locale: 'zh-CN' })
  try {
    await begin(shortPage, 9, '赵云', 2)
    const board = shortPage.locator('[data-battle-table]')
    const toolbar = board.locator('.classic-hand-toolbar')
    await button(toolbar, '龙胆').click()
    await assertPlayedCard(board, 1, '杀', '8♠')
    await assertToolbarPosition(board)
    await board.locator('[data-hand-gallery] button:enabled').first().click()
    await button(toolbar, '确认').click()
    await board.locator('[data-resolving-card="1"]').waitFor({ state: 'detached' })
    console.log('Two-seat short landscape played face and controls passed')
  } finally {
    await shortPage.close()
  }

  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, locale: 'zh-CN' })
  try {
    await begin(page, 1316, '赵云')
    const board = page.locator('[data-battle-table]')
    const toolbar = board.locator('.classic-hand-toolbar')
    const gallery = board.locator('[data-hand-gallery]')
    const dodge = button(toolbar, '使用闪')
    await dodge.and(toolbar.locator('button:enabled')).waitFor()
    const before = Number(await board.getAttribute('data-revision'))
    await button(toolbar, '龙胆').click()
    assert.equal(await gallery.getByRole('button', { name: /^杀 方片 7 / }).isEnabled(), true)
    assert.equal(await gallery.getByRole('button', { name: /^杀 梅花 7 / }).isEnabled(), true)
    await button(toolbar, '取消').click()
    await dodge.click()
    assert.equal(Number(await board.getAttribute('data-revision')), before)
    assert.equal(await gallery.locator('button:enabled').count(), 2)
    const chosen = gallery.locator('[data-card-id="33"]')
    await chosen.click()
    assert.equal(await chosen.getAttribute('aria-pressed'), 'true')
    await button(toolbar, '确认').click()
    assert.equal(await chosen.count(), 0)
    assert.equal(await gallery.locator('[data-card-id="36"]').count(), 1)
    assert.ok(Number(await board.getAttribute('data-revision')) > before)
    console.log('Multiple Shan choices and distinct Longdan suit labels passed')
  } finally {
    await page.close()
  }
} finally {
  await browser.close()
}
