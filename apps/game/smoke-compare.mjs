// Start the ordinary Vite dev server. Supply a separate duel-web build with qa-compare enabled.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const url = process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420'
const bindings = process.env.MAK_COMPARE_BINDINGS
assert.ok(bindings, 'Set MAK_COMPARE_BINDINGS to the qa-compare wasm-bindgen web output')
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')
const evidence =
  process.env.MAK_SMOKE_SCREENSHOTS && path.resolve(process.env.MAK_SMOKE_SCREENSHOTS)
if (evidence) {
  assert.ok(
    evidence !== root && !evidence.startsWith(root + path.sep),
    'Save screenshots outside the repo',
  )
  await fs.mkdir(evidence, { recursive: true })
}
const json = async (file) => JSON.parse(await fs.readFile(path.join(root, file), 'utf8'))
const [fixtureJs, golden, deck, roster] = await Promise.all([
  fs.readFile(path.join(bindings, 'duel.js'), 'utf8'),
  json('tests/reference/compare.json'),
  json('packages/duel-core/data/standard-deck.json'),
  json('packages/duel-core/data/standard-2008-generals.json'),
])
// These deals give the reference bids directly from the 104-card deck. The bot chooses its own bid.
const cases = [
  {
    sample: 'modified-win',
    seed: 9282,
    humanIsTarget: false,
    reduced: false,
    width: 1280,
    height: 720,
  },
  { sample: 'tie', seed: 47319, humanIsTarget: true, reduced: true, width: 640, height: 272 },
  {
    sample: 'loss',
    seed: 233835,
    humanIsTarget: true,
    pauseBeforeResult: true,
    reduced: false,
    width: 844,
    height: 390,
  },
]
const browser = await chromium.launch({ executablePath: process.env.MAK_QA_BROWSER })
const errors = []
const button = (page, name) => page.getByRole('button', { name, exact: true })
const handCard = (page, id) => page.locator(`[data-hand] [data-card-id="${id}"]`)
const shot = (page, name) =>
  evidence && page.screenshot({ path: path.join(evidence, `compare-${name}.png`) })
const projections = (page) =>
  page.evaluate(() =>
    Array.from({ length: 9 }, (_, i) =>
      JSON.parse(i < 8 ? window.compareDuel.view(i) : window.compareDuel.view()),
    ),
  )

async function start(test, fixture) {
  const { width, height, reduced, seed, humanIsTarget } = test
  const page = await browser.newPage({ viewport: { width, height }, locale: 'zh-CN' })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type()))
      errors.push(`${message.location().url} ${message.text()}`)
  })
  if (fixture) {
    const factory = `Duel.classic = (...args) => (window.compareDuel = classicCompareFixture(...args, ${humanIsTarget}, ${!!test.pauseBeforeResult}))`
    await page.route('**/src/generated/duel/duel.js*', (route) =>
      route.fulfill({ contentType: 'text/javascript', body: `${fixtureJs}\n${factory}` }),
    )
    await page.route('**/src/generated/duel/duel_bg.wasm*', (route) =>
      route.fulfill({ contentType: 'application/wasm', path: path.join(bindings, 'duel_bg.wasm') }),
    )
  }
  await page.addInitScript(
    ({ seed, reduced }) => {
      Math.random = () => (seed + 0.1) / 0x7fffffff
      localStorage.setItem('mak-reduced-motion', String(reduced))
    },
    { seed, reduced },
  )
  await page.goto(url)
  await button(page, '开始游玩').click()
  assert.equal(await page.locator('.mode-feature').count(), 1)
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  assert.equal(await page.locator('.pick-grid button').count(), 25)
  await button(page, '张飞').click()
  await page.locator('select').selectOption('8')
  await page.clock.install({ time: new Date('2026-01-01T12:00:00Z') })
  await page.clock.pauseAt(new Date('2026-01-01T12:00:05Z'))
  await button(page, '开始对局').click()
  await page.waitForURL('**/battle')
  await page.locator('[data-battle-table]').waitFor()
  assert.equal(await page.title(), '随心杀')
  assert.equal(await page.locator('[data-seat]').count(), 8)
  assert.equal(await page.locator('vite-error-overlay').count(), 0)
  return page
}

function verifyPrivate(views, seats, cards, chooser) {
  for (const [viewer, view] of views.entries()) {
    assert.equal(view.waitingFor, chooser)
    if (viewer === chooser) {
      assert.deepEqual(view.choice.compare, { initiator: seats[0], target: seats[1] })
      assert.deepEqual(
        view.choice.branches[0].cards,
        view.seats[chooser].hand.map((card) => card.id),
      )
    } else {
      assert.equal(view.choice, null)
    }
    assert.deepEqual(view.actions, [])
    assert.deepEqual(view.privateChoiceCards ?? [], [])
    for (const [index, card] of cards.entries()) {
      const owner = seats[index]
      assert.equal(
        view.cardFaces.some((f) => f.id === card.id),
        viewer === owner,
      )
      assert.equal(view.seats[owner].hand?.some((c) => c.id === card.id) ?? false, viewer === owner)
      assert.ok(!view.seats[owner].knownHand?.some((c) => c.id === card.id))
      assert.ok(![...view.resolving, ...view.discard].some((c) => c.id === card.id))
      for (const event of view.events) {
        assert.notEqual(event.card?.id, card.id)
        assert.ok(!event.cards?.some((c) => c.id === card.id))
      }
    }
    assert.ok(
      !view.events.some((e) => ['compareReveal', 'compareResult', 'showCards'].includes(e.type)),
    )
  }
}

async function chooseInHand(page, choice, bid, opponent) {
  assert.match(
    await page.locator('.classic-center h1').innerText(),
    new RegExp(`${opponent}.*拼点|拼点.*${opponent}`),
  )
  const confirm = button(page, '确认')
  assert.ok(await confirm.isDisabled())
  assert.equal(
    await page.locator('[data-hand] button:enabled').count(),
    choice.branches[0].cards.length,
  )
  const alternate = choice.branches[0].cards.find((id) => id !== bid.id)
  await handCard(page, alternate).click()
  await handCard(page, bid.id).click()
  assert.equal(await handCard(page, alternate).getAttribute('aria-pressed'), 'false')
  assert.equal(await handCard(page, bid.id).getAttribute('aria-pressed'), 'true')
  assert.ok(await confirm.isEnabled())
  await confirm.click()
}

async function runCase(test, baseline) {
  const { humanIsTarget, sample } = test
  const reference = golden.trials.find((t) => t.sample === sample)
  const cards = reference.hands.map(([[kind, suit, rank]]) => {
    const card = deck.find((c) => c.kind === kind && c.suit === suit && c.rank === rank)
    assert.ok(card)
    return card
  })
  const seats = humanIsTarget ? [1, 0] : [0, 1]
  const [initiator, target] = seats
  const page = await start(test, true)
  const initial = await projections(page)
  for (const [id, view] of initial.entries()) {
    assert.deepEqual(
      view.seats,
      baseline[id].seats,
      'every seat gets the regular deal, heroes and roles',
    )
    assert.equal(view.deckCount, baseline[id].deckCount)
  }
  assert.equal(
    initial[0].deckCount + initial[0].seats.reduce((n, seat) => n + seat.handCount, 0),
    104,
  )
  const names = initial[0].seats.map((seat) => roster.find((hero) => hero.id === seat.hero).name)
  verifyPrivate(initial, seats, cards, initiator)
  assert.equal(await button(page, '拼点牌').count(), 0)
  if (humanIsTarget) {
    assert.equal(await page.locator('[data-hand] button:enabled').count(), 0)
  } else {
    await page.clock.runFor(1800)
    const choice = initial[0].choice
    const forged = {
      version: initial[0].version,
      seat: target,
      action: {
        type: 'select',
        event: choice.event,
        branch: choice.branches[0].id,
        cards: [cards[0].id],
        targets: [],
      },
    }
    await assert.rejects(
      page.evaluate((command) => window.compareDuel.submit(JSON.stringify(command)), forged),
      /WrongActor/,
    )
    await shot(page, `${sample}-private`)
    await chooseInHand(page, choice, cards[0], names[target])
  }
  if (humanIsTarget) await page.clock.runFor(2200)
  const waiting = await projections(page)
  verifyPrivate(waiting, seats, cards, target)
  assert.equal(waiting[0].version, initial[0].version + 1)
  assert.equal(
    await page.locator('[data-compare-bid]').count(),
    0,
    'no public face enters the DOM between bids',
  )
  assert.ok(!(await page.locator('.classic-log ol').textContent()).includes('拼点亮牌'))
  if (humanIsTarget) {
    await shot(page, `${sample}-private`)
    await chooseInHand(page, waiting[0].choice, cards[1], names[initiator])
  } else {
    assert.equal(await page.locator('[data-hand] button:enabled').count(), 0)
    await page.clock.runFor(1600)
  }
  const ids = cards.map((c) => c.id)
  const panel = page.locator('#compare-panel')
  let beforeResult
  if (test.pauseBeforeResult) {
    beforeResult = await projections(page)
    for (const view of beforeResult) {
      const revealed = view.events.filter(
        (e) => e.type === 'compareReveal' && e.status === 'started',
      )
      assert.deepEqual(
        revealed.map((e) => e.cards.map((c) => c.id)),
        [ids],
      )
      assert.ok(!view.events.some((e) => e.type === 'compareResult'))
      assert.deepEqual(
        view.resolving.map((c) => c.id),
        ids,
      )
    }
    assert.equal(await panel.locator('h2').innerText(), '拼点亮牌')
    assert.deepEqual(
      await panel
        .locator('[data-compare-bid]')
        .evaluateAll((faces) => faces.map((face) => Number(face.dataset.compareBid))),
      ids,
    )
    assert.equal(
      await panel.locator('figcaption b').count(),
      0,
      'effective points wait for the public outcome',
    )
    await shot(page, `${sample}-before-result`)
    await page.clock.runFor(1000)
    await button(page, '继续结算拼点').click()
    await button(page, '确认').click()
  }
  const publicViews = await projections(page)
  const winner =
    reference.result.winner === null ? null : seats[reference.result.winner === 'A' ? 0 : 1]
  for (const [viewer, view] of publicViews.entries()) {
    const events = [...(beforeResult?.[viewer].events ?? []), ...view.events]
    const announcements = events.filter(
      (e) => e.status === 'started' && ['compareReveal', 'compareResult'].includes(e.type),
    )
    assert.deepEqual(
      announcements.map((e) => e.type),
      ['compareReveal', 'compareResult'],
    )
    const [reveal, { result }] = announcements
    assert.deepEqual([reveal.seat, reveal.target], seats)
    assert.deepEqual(
      reveal.cards.map((c) => c.id),
      ids,
    )
    assert.equal(reveal.parent, result.event)
    assert.deepEqual(result.cards, reveal.cards)
    assert.deepEqual(result.participants, { initiator, target })
    assert.deepEqual(result.numbers, reference.result.numbers)
    assert.equal(result.won, reference.result.bool)
    assert.equal(result.tie, reference.result.tie)
    assert.equal(result.winner, winner)
    assert.equal(view.version, waiting[0].version + (test.pauseBeforeResult ? 2 : 1))
    assert.deepEqual(view.resolving, [])
    for (const card of cards) {
      assert.ok(view.discard.some((c) => c.id === card.id))
      assert.ok(
        view.cardFaces.some(
          (f) => f.id === card.id && f.rank === card.rank && f.suit === card.suit,
        ),
      )
    }
  }
  const bids = panel.locator('[data-compare-bid]')
  assert.ok(await panel.isVisible())
  assert.equal(await bids.count(), 2)
  const outcome = winner === null ? '拼点平局' : `${names[winner]}拼点获胜`
  assert.equal(await panel.locator('h2').innerText(), outcome)
  for (const [i, card] of cards.entries()) {
    const name = card.kind === 'tao' ? '桃' : '闪'
    const rank = `${card.rank}${card.suit === 'heart' ? '♥' : '♦'}`
    const bid = bids.nth(i)
    assert.equal(Number(await bid.getAttribute('data-compare-bid')), card.id)
    assert.equal(await bid.locator('strong').innerText(), name)
    assert.equal(await bid.locator('.card-rank').innerText(), rank)
    assert.equal(
      await panel.locator('figcaption').nth(i).innerText(),
      `${names[seats[i]]} · ${reference.result.numbers[i]}点`,
    )
    assert.ok(
      await bid.locator('img').evaluate(async (img, kind) => {
        await img.decode()
        return img.naturalWidth > 0 && img.src.endsWith(`/card/${kind}.png`)
      }, card.kind),
    )
  }
  const [firstBox, secondBox] = await Promise.all([
    bids.nth(0).boundingBox(),
    bids.nth(1).boundingBox(),
  ])
  assert.ok(firstBox.x >= 0 && firstBox.x + firstBox.width <= secondBox.x)
  assert.ok(
    secondBox.x + secondBox.width <= test.width && secondBox.y + secondBox.height <= test.height,
  )
  await shot(page, `${sample}-reveal`)
  await page.clock.runFor(4200)
  assert.equal(await button(page, '拼点牌').getAttribute('aria-expanded'), 'false')
  await button(page, '结束出牌').click()
  await page.locator('.classic-log summary').click()
  const lines = await page.locator('.classic-log li').allInnerTexts()
  const scored = lines.filter((line) => line.includes(outcome))
  assert.equal(scored.length, 1)
  for (const [index, n] of reference.result.numbers.entries())
    assert.ok(scored[0].includes(`${names[seats[index]]} ${n}点`))
  const revealed = lines.filter((line) => line.includes('拼点亮牌'))
  assert.equal(revealed.length, 1)
  assert.ok(revealed[0].includes('桃') && revealed[0].includes('闪'))
  await page.locator('.classic-log summary').click()
  await button(page, '拼点牌').click()
  assert.ok(
    (await bids.first().isVisible()) && (await bids.last().isVisible()),
    'reopening after another command preserves both printed cards',
  )
  await page.close()
  return {
    sample,
    seed: test.seed,
    seats,
    cards: ids,
    numbers: reference.result.numbers,
    winner,
    reduced: test.reduced,
  }
}

try {
  const ordinary = await start(cases[0], false)
  assert.equal(await button(ordinary, '拼点牌').count(), 0)
  const baselines = await ordinary.evaluate(
    async (seeds) => {
      const mod = await import('/src/generated/duel/duel.js')
      if (Object.hasOwn(mod, 'classicCompareFixture'))
        throw new Error('Ordinary bindings exported the QA fixture')
      await mod.default()
      return seeds.map((seed) => {
        const normal = mod.Duel.classic(seed, 0, 'zhangfei', 8)
        try {
          return Array.from({ length: 9 }, (_, id) => {
            const { seats, deckCount } = JSON.parse(id < 8 ? normal.view(id) : normal.view())
            return { seats, deckCount }
          })
        } finally {
          normal.free()
        }
      })
    },
    cases.map((test) => test.seed),
  )
  await ordinary.close()
  const results = []
  for (const [i, test] of cases.entries()) results.push(await runCase(test, baselines[i]))
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ results, errors, evidence }))
} finally {
  await browser.close()
}
