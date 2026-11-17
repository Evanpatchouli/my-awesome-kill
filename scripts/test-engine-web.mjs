// Exercise the shipped WASM interface, including a rejected command and private response.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import init, { Duel } from '../apps/game/src/generated/duel/duel.js'

const binary = await readFile(
  new URL('../apps/game/src/generated/duel/duel_bg.wasm', import.meta.url),
)
await init({ module_or_path: binary })
for (const dodge of [true, false]) {
  const game = new Duel(
    JSON.stringify({
      activeSeat: 3,
      seats: [
        { id: 3, hp: 4, hand: [{ id: 10, kind: 'sha' }] },
        { id: 7, hp: 3, hand: [{ id: 20, kind: 'shan' }] },
        { id: 9, hp: 4, hand: [] },
      ],
    }),
  )
  try {
    const view = (seat) => JSON.parse(game.view(seat))
    const submit = (version, seat, action) => game.submit(JSON.stringify({ version, seat, action }))
    submit(0, 3, { type: 'playSha', card: 10, target: 7 })
    const waiting = view(7)
    assert.deepEqual(waiting.choice.cards, [20])
    assert.equal(view(9).choice, null)
    assert.equal(view(9).seats[1].hand, null)
    assert.deepEqual(
      waiting.resolving.map((c) => c.id),
      [10],
    )
    const answer = { type: 'choose', event: waiting.choice.event, card: dodge ? 20 : null }
    assert.throws(() => submit(0, 7, answer))
    assert.deepEqual(view(7), waiting)
    submit(1, 7, answer)
    const final = view(3)
    assert.equal(final.seats[1].hp, dodge ? 3 : 2)
    assert.deepEqual(final.resolving, [])
    assert.deepEqual(final.discard.map((c) => c.id).sort(), dodge ? [10, 20] : [10])
  } finally {
    game.free()
  }
}
console.log(
  'WASM conformance passed: three seats, private response, stale version, damage/dodge, card cleanup.',
)

// Reduce the real upstream trace to the shared milestones implemented by this slice.
// The full upstream hook log remains in the golden file for later migrations.
const golden = JSON.parse(
  await readFile(new URL('../tests/reference/sha-no-shan.json', import.meta.url)),
)
const referenceHooks = new Map([
  ['useCardBefore', 'use'],
  ['chooseToUseBefore', 'response'],
  ['damage', 'damage'],
  ['changeHp', 'hp'],
  ['useCardAfter', 'use-finished'],
  ['cardsDiscardEnd', 'discard'],
])
const expected = golden.events.flatMap(([hook]) =>
  referenceHooks.has(hook) ? [referenceHooks.get(hook)] : [],
)
const seats = Array.from({ length: golden.setup.players }, (_, id) => ({
  id,
  hp: 4,
  hand: id === 0 ? [{ id: 0, kind: 'sha' }] : [],
}))
const fixture = new Duel(JSON.stringify({ seats, activeSeat: 0 }))
try {
  fixture.submit(
    JSON.stringify({ version: 0, seat: 0, action: { type: 'playSha', card: 0, target: 1 } }),
  )
  const pending = JSON.parse(fixture.view(1))
  const rows = [...pending.events]
  fixture.submit(
    JSON.stringify({
      version: pending.version,
      seat: 1,
      action: { type: 'choose', event: pending.choice.event, card: null },
    }),
  )
  const after = JSON.parse(fixture.view(0))
  rows.push(...after.events)
  const rustEvents = new Map([
    ['useSha/started', 'use'],
    ['chooseShan/started', 'response'],
    ['damage/started', 'damage'],
    ['changeHp/started', 'hp'],
    ['useSha/finished', 'use-finished'],
    ['discard/finished', 'discard'],
  ])
  assert.deepEqual(
    rows.flatMap((e) => rustEvents.get(e.type + '/' + e.status) ?? []),
    expected,
  )
  const hp = rows.find((e) => e.type === 'changeHp')
  assert.equal(hp.before, golden.events.find((row) => row[0] === 'damage')[5])
  assert.equal(hp.after, golden.events.find((row) => row[0] === 'changeHp')[5])
  assert.equal(after.seats[1].hp, golden.final.targetHp)
  assert.equal(after.seats[1].alive, golden.final.targetAlive)
  assert.equal(
    after.discard.some((c) => c.id === 0),
    golden.final.inDiscard,
  )
  console.log(
    'WASM / pinned upstream: six common milestones match, including damage before HP change and final discard.',
  )
} finally {
  fixture.free()
}
