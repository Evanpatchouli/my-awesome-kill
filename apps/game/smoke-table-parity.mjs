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
const right = ([x, , width]) => x + width
const bottom = ([, y, , height]) => y + height
const contains = (outer, inner) =>
  inner[0] >= outer[0] - 1 && inner[1] >= outer[1] - 1 &&
  right(inner) <= right(outer) + 1 && bottom(inner) <= bottom(outer) + 1
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
    equipmentRows: [...board.querySelectorAll('.classic-self-equipment .equipment-row')].map(rect),
    skills: rect(board.querySelector('.classic-own-skills')),
    self: rect(board.querySelector('.classic-self')),
    prompt: rect(board.querySelector('.classic-center h1')),
    controls: rect(board.querySelector('.classic-controls')),
    health: [...board.querySelectorAll('[data-seat] .health')].map((meter) => ({
      meter: rect(meter),
      card: rect(meter.closest('[data-seat]')),
      direction: getComputedStyle(meter).flexDirection,
    })),
    scroll: [window.scrollX, window.scrollY, board.scrollLeft, board.scrollTop],
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
      const screen = [0, 0, viewport.width, viewport.height]
      for (const [index, { position, rect }] of opening.seats.entries()) {
        assert.ok(contains(screen, rect), `${label} seat ${position} visible [${rect}]`)
        const previous = opening.seats[index - 1]
        if (previous) {
          assert.ok(right(rect) <= previous.rect[0] + 1,
            `${label} seat ${position} [${rect}] is left of seat ${previous.position} [${previous.rect}]`)
        }
      }
      const horizontalOverlap = ([left, , width], [otherLeft, , otherWidth]) =>
        left < otherLeft + otherWidth && otherLeft < left + width
      const seatBottom = Math.max(...opening.seats.filter(({ rect }) =>
        horizontalOverlap(rect, opening.prompt)).map(({ rect }) => bottom(rect)))
      assert.ok(
        seatBottom <= opening.prompt[1] && bottom(opening.prompt) <= opening.hand[1],
        `${label} prompt between opponents and hand`,
      )
      const { equipment, equipmentRows, skills, self, hand } = opening
      assert.equal(equipmentRows.length, 4, label)
      assert.ok(contains(screen, equipment), `${label} equipment column visible [${equipment}]`)
      for (const row of equipmentRows) {
        assert.ok(contains(equipment, row), `${label} equipment row [${row}] visible in [${equipment}]`)
      }
      assert.ok(right(equipment) <= hand[0] + 1 && right(hand) <= skills[0] + 1 &&
        right(skills) <= self[0] + 1,
        `${label} equipment [${equipment}], hand [${hand}], skills [${skills}], self [${self}] ordered left to right`)
      assert.ok(Math.abs(equipment[1] - self[1]) <= 1 && Math.abs(bottom(equipment) - bottom(self)) <= 1,
        `${label} equipment [${equipment}] and self [${self}] fill the same dock height`)
      assert.ok(contains(screen, self) && contains(screen, hand), `${label} self and hand visible`)
      assert.ok(bottom(opening.controls) <= hand[1], `${label} controls above hand`)
      assert.equal(opening.health.length, 8, label)
      for (const { meter, card, direction } of opening.health) {
        assert.match(direction, /^row/, `${label} health runs horizontally`)
        assert.ok(contains(card, meter), `${label} health [${meter}] inside portrait [${card}]`)
      }
      assert.deepEqual(opening.scroll, [0, 0, 0, 0], `${label} board starts without scrolling`)
      if (evidence) await page.screenshot({ path: path.join(evidence, `table-parity-${label}.png`) })

      const revision = Number(await board.getAttribute('data-revision'))
      const cardId = await card.getAttribute('data-card-id')
      const played = board.locator(`[data-hand-gallery] [data-card-id="${cardId}"]`)
      await played.click()
      assert.equal(await played.getAttribute('aria-pressed'), 'true', label)
      const selectedRankVisible = await played.evaluate((element) => {
        const rank = element.querySelector('.card-rank').getBoundingClientRect()
        const gallery = element.closest('[data-hand-gallery]').getBoundingClientRect()
        return rank.top >= gallery.top && rank.bottom <= gallery.bottom
      })
      assert.ok(selectedRankVisible, `${label} lifted card rank stays inside the gallery`)
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
