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
    hand: rect(board.querySelector('[data-hand-gallery]')),
    equipment: rect(board.querySelector('.classic-self-equipment')),
    skills: rect(board.querySelector('.classic-own-skills')),
    self: rect(board.querySelector('.classic-self')),
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
      const card = page.getByRole('button', { name: /^杀 / }).and(page.locator('[data-hand-gallery] button:enabled')).first()
      await card.waitFor()
      const opening = await measure(page)
      assert.equal(await board.locator('[data-seat]').count(), 8, label)
      assert.deepEqual(opening.seats.map((seat) => seat.position), [1, 2, 3, 4, 5, 6, 7], label)
      const horizontalOverlap = ([left, , width], [otherLeft, , otherWidth]) =>
        left < otherLeft + otherWidth && otherLeft < left + width
      const seatBottom = Math.max(...opening.seats.filter(({ rect }) =>
        horizontalOverlap(rect, opening.prompt)).map(({ rect }) => rect[1] + rect[3]))
      assert.ok(
        seatBottom <= opening.prompt[1] && opening.prompt[1] + opening.prompt[3] <= opening.hand[1],
        `${label} prompt between opponents and hand`,
      )
      const equipmentRows = await board.locator('.classic-self-equipment .equipment-row').evaluateAll((rows) =>
        rows.map((row) => {
          const { top, bottom, height } = row.getBoundingClientRect()
          return { top, bottom, height }
        }),
      )
      assert.equal(equipmentRows.length, 4, label)
      assert.ok(equipmentRows.every(row => Math.abs(row.height - equipmentRows[0].height) <= 1), `${label} equipment rows have equal height`)
      near([equipmentRows[0].top, equipmentRows.at(-1).bottom],
        [opening.self[1], opening.self[1] + opening.self[3]], `${label} slots fill the portrait height`)
      const insets = await board.locator('.classic-bottom').evaluate((dock) => {
        const { top, bottom } = dock.getBoundingClientRect()
        const { top: equipmentTop, bottom: equipmentBottom } = dock.querySelector('.classic-self-equipment').getBoundingClientRect()
        return [equipmentTop - top - parseFloat(getComputedStyle(dock).borderTopWidth), bottom - equipmentBottom]
      })
      near(insets, [insets[1], insets[1]], `${label} equipment insets are symmetric`)
      assert.ok(opening.equipment[0] + opening.equipment[2] <= opening.hand[0] &&
        opening.hand[0] + opening.hand[2] <= opening.skills[0] &&
        opening.skills[0] + opening.skills[2] <= opening.self[0], `${label} bottom zones ordered left to right`)
      assert.ok(opening.controls[1] + opening.controls[3] <= opening.hand[1], `${label} controls above hand`)
      const health = await board.locator('.classic-self .health').evaluate((element) => {
        const meter = element.getBoundingClientRect()
        const portrait = element.closest('.self-select').querySelector('img').getBoundingClientRect()
        return { direction: getComputedStyle(element).flexDirection, meter: [meter.left, meter.top, meter.right, meter.bottom], portrait: [portrait.left, portrait.top, portrait.right, portrait.bottom] }
      })
      assert.match(health.direction, /^column/)
      assert.ok(health.meter[0] >= health.portrait[0] && health.meter[1] >= health.portrait[1] &&
        health.meter[2] <= health.portrait[2] && health.meter[3] <= health.portrait[3], `${label} health inside portrait`)
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
      const played = board.locator(`[data-hand-gallery] [data-card-id="${cardId}"]`)
      await played.click()
      assert.equal(await played.getAttribute('aria-pressed'), 'true', label)
      assert.ok(await board.locator('[data-hand-rule]').isVisible(), `${label} selected card rule visible`)
      await button(page, '取消').click()
      assert.equal(await played.getAttribute('aria-pressed'), 'false', `${label} cancel clears selection`)
      assert.equal(await board.locator('[data-hand-rule]').count(), 0, `${label} cancel clears rule`)
      await played.click()
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
