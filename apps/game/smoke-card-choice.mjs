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

try {
  for (const viewport of [{ width: 1280, height: 720 }, { width: 640, height: 272 }]) {
    const page = await browser.newPage({ viewport, locale: 'zh-CN' })
    const errors = []
    page.on('pageerror', error => errors.push(error.message))
    page.on('console', message => {
      if (['error', 'warning'].includes(message.type())) errors.push(message.text())
    })
    try {
      await page.addInitScript(() => {
        Math.random = () => 20.1 / 0x7fffffff
        localStorage.setItem('mak-reduced-motion', 'true')
      })
      assert.ok((await page.goto(process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'))?.ok())
      assert.equal(await page.title(), '随心杀')
      await page.getByRole('button', { name: /开始\s*游玩/ }).click()
      await page.getByRole('button', { name: /经典三国杀/ }).click()
      await button(page, '张飞').click()
      await page.locator('select').selectOption('8')
      await button(page, '开始对局').click()
      const board = page.locator('[data-battle-table]')
      const gallery = board.locator('[data-hand-gallery]')
      const dismantle = gallery.getByRole('button', { name: /^过河拆桥 / }).first()
      await dismantle.click()
      assert.match(await board.locator('[data-hand-rule]').innerText(), /过河拆桥.*弃置/s)
      if (evidence) await page.screenshot({ path: path.join(evidence, `selected-${viewport.width}.png`) })

      const target = board.locator('.classic-seat .seat-select:enabled').first()
      await target.click()
      await button(page, '确认').click()
      const central = board.locator('[data-card-choice]')
      const hidden = central.locator('.card-back')
      const first = hidden.first()
      await first.waitFor()
      assert.ok(await hidden.count() >= 2)
      assert.equal(await hidden.locator('.card-art, .card-rank, strong').count(), 0)
      assert.equal(await gallery.locator('button:enabled').count(), 0)
      assert.equal(await button(central, '确认').isDisabled(), true)
      await first.click()
      assert.equal(await first.getAttribute('aria-pressed'), 'true')
      const rect = await central.boundingBox()
      assert.ok(rect.y >= 0 && rect.y + rect.height <= viewport.height + 1)
      const confirm = button(central, '确认')
      const confirmBox = await confirm.boundingBox()
      assert.ok(confirmBox.y + confirmBox.height <= rect.y + rect.height + 1, 'confirm is visible without scrolling')
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
  }
} finally {
  await browser.close()
}
