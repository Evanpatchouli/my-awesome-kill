import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const url = process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'
const root = fileURLToPath(new URL('../..', import.meta.url))
const evidence = process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence) {
  const relative = path.relative(root, evidence)
  assert.ok(
    relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative),
    'Save screenshots outside the repository',
  )
  await fs.mkdir(evidence, { recursive: true })
}

const viewports = [{ width: 1280, height: 720 }, { width: 844, height: 390 }, { width: 640, height: 272 }]
// Measured on the pinned noname revision c7327094, standard eight-seat identity, long2/simple.
const opponents = [[1121.6, 129.2], [941, 68.3], [760.5, 46], [580, 46], [399.4, 46], [218.9, 68.3], [38.4, 129.2]]
const near = (actual, expected, label) => {
  assert.equal(actual.length, expected.length, label)
  actual.forEach((value, i) => assert.ok(Math.abs(value - expected[i]) <= 1,
    `${label} coordinate ${i}: ${value}, expected ${expected[i]}`))
}
const button = (page, name) => page.getByRole('button', { name, exact: true })
const targets = (page) => page.locator('.classic-seat .seat-select:enabled')
const measure = (page) => page.locator('[data-battle-table]').evaluate((board) => {
  const rect = (element) => {
    const { x, y, width, height } = element.getBoundingClientRect()
    return [x, y, width, height].map((value) => Math.round(value * 10) / 10)
  }
  return {
    seats: [...board.querySelectorAll('.classic-seat[data-position]')].map((seat) => ({
      position: Number(seat.dataset.position), rect: rect(seat), face: rect(seat.querySelector('img')),
    })),
    hand: rect(board.querySelector('[data-hand]')),
    prompt: rect(board.querySelector('.classic-center h1')),
    controls: rect(board.querySelector('.classic-controls')),
    scroll: [window.scrollY, board.scrollTop],
  }
})

const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
try {
  for (const viewport of viewports) {
    const label = `${viewport.width}x${viewport.height}`
    const page = await browser.newPage({ viewport, locale: 'zh-CN' })
    const errors = []
    page.on('pageerror', (error) => errors.push(error.message))
    page.on('console', (message) => {
      if (message.type() === 'error') errors.push(message.text())
    })
    try {
      await page.addInitScript(() => {
        // A normal shuffle with a playable first-turn attack makes the same UI path repeatable.
        Math.random = () => 317.1 / 0x7fffffff
      })
      assert.ok((await page.goto(url))?.ok(), label)
      await page.getByRole('button', { name: /开始\s*游玩/ }).click()
      await page.getByRole('button', { name: /经典三国杀/ }).click()
      await button(page, '关羽').click()
      await page.locator('select').selectOption('8')
      await button(page, '开始对局').click()
      const board = page.locator('[data-battle-table]')
      await board.waitFor()
      const card = page.getByRole('button', { name: /^杀 / }).and(page.locator('[data-hand] button:enabled')).first()
      await card.waitFor()
      const opening = await measure(page)
      assert.equal(await board.locator('[data-seat]').count(), 8, label)
      assert.deepEqual(opening.seats.map((seat) => seat.position), [1, 2, 3, 4, 5, 6, 7], label)
      const seatBottom = Math.max(...opening.seats.map(({ rect }) => rect[1] + rect[3]))
      assert.ok(
        seatBottom <= opening.prompt[1] && opening.prompt[1] + opening.prompt[3] <= opening.hand[1],
        `${label} prompt between opponents and hand`,
      )
      assert.ok(opening.hand[0] + opening.hand[2] <= opening.controls[0], `${label} controls beside hand`)
      if (viewport.width === 1280) {
        for (const { position, rect, face } of opening.seats) {
          const [x, y] = opponents[position - 1]
          near(rect, [x, y, 120, 200], `seat ${position}`)
          near(face, [x + 3, y + 3, 114, 194], `seat ${position} portrait`)
        }
      }
      if (evidence) await page.screenshot({ path: path.join(evidence, `table-parity-${label}.png`) })

      const revision = Number(await board.getAttribute('data-revision'))
      const cardId = await card.getAttribute('data-card-id')
      const played = board.locator(`[data-hand] [data-card-id="${cardId}"]`)
      await played.click()
      assert.equal(await played.getAttribute('aria-pressed'), 'true', label)
      const legalPositions = await targets(page).evaluateAll((buttons) =>
        buttons.map((item) => Number(item.closest('[data-position]').dataset.position)),
      )
      assert.deepEqual(legalPositions, [1, 7], label)
      const otherTarget = board.locator('.classic-seat[data-position="7"] .seat-select')
      await otherTarget.click()
      assert.equal(await otherTarget.getAttribute('aria-pressed'), 'true', label)
      await played.click()
      assert.equal(await played.getAttribute('aria-pressed'), 'false', label)
      assert.equal(await targets(page).count(), 0, `${label} clears target highlights`)
      await played.click()
      const targeting = await measure(page)
      assert.deepEqual(targeting.scroll, opening.scroll, `${label} board stays in place`)
      assert.deepEqual(targeting.seats, opening.seats, `${label} selection keeps seat layout stable`)

      const target = board.locator('.classic-seat[data-position="1"] .seat-select')
      const hit = await target.evaluate((element) => {
        const { x, y, width, height } = element.getBoundingClientRect()
        const lowerPoint = { x: width / 2, y: height - 8 }
        const lowerHit = document.elementFromPoint(x + lowerPoint.x, y + lowerPoint.y)
        return {
          inside: x >= 0 && y >= 0 && x + width <= innerWidth && y + height <= innerHeight,
          center: element.contains(document.elementFromPoint(x + width / 2, y + height / 2)),
          lower: element.contains(lowerHit),
          lowerPoint,
          interceptedBy: lowerHit?.closest('.classic-zones')?.className ?? lowerHit?.tagName,
        }
      })
      assert.ok(hit.inside && hit.center, `${label} target visible and clickable`)
      assert.ok(hit.lower, `${label} target lower edge hit ${hit.interceptedBy}`)
      await target.click({ position: hit.lowerPoint })
      const confirm = button(page, '确认')
      assert.ok(await confirm.isEnabled(), label)
      await confirm.click()
      await played.waitFor({ state: 'detached' })
      assert.ok(Number(await board.getAttribute('data-revision')) > revision, `${label} engine accepted play`)
      assert.equal(await page.locator('vite-error-overlay,.game-error').count(), 0, label)
      assert.deepEqual(errors, [], label)
      console.log(JSON.stringify({
        viewport: label,
        seats: opening.seats,
        prompt: opening.prompt,
        hand: opening.hand,
        controls: opening.controls,
        scroll: targeting.scroll,
        revision: [revision, Number(await board.getAttribute('data-revision'))],
      }))
    } finally {
      await page.context().close()
    }
  }
} finally {
  await browser.close()
}
