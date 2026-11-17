import { describe, expect, it } from 'vitest'
import { standardDeck, type Card } from '../src/cards'
import { judgePhase, playTrick, trickPlayable } from '../src/tricks'
import type { Game, Seat, Option } from '../src/model'

let nextId = 1000
const card = (kind: string): Card => ({ ...standardDeck().find((c) => c.kind === kind)!, id: nextId++ })
function fixture() {
  const players = [0, 1].map(() => ({
    name: '角色',
    hero: 'zhangfei',
    hp: 3,
    maxHp: 4,
    hand: [] as Card[],
    equipment: [] as Card[],
    judges: [] as Card[],
  }))
  const table: Card[] = [],
    discard: Card[] = [],
    deck: Card[] = []
  const actions: Option[][] = []
  const damage: number[] = []
  let draws = 0,
    insights = 0
  const g = {
    revealed: new Set<Card>(),
    players,
    active: 0 as Seat,
    skipPlay: false,
    turn: 1,
    other: (s: Seat) => (1 - s) as Seat,
    move: (c: Card, zone: string, s?: Seat) => {
      for (const p of players)
        for (const a of [p.hand, p.equipment, p.judges]) {
          const i = a.indexOf(c)
          if (i >= 0) a.splice(i, 1)
        }
      for (const a of [table, discard, deck]) {
        const i = a.indexOf(c)
        if (i >= 0) a.splice(i, 1)
      }
      if (zone === 'table') table.push(c)
      else if (zone === 'discard') discard.push(c)
      else players[s!][zone as 'hand' | 'equipment' | 'judges'].push(c)
    },
    reveal: () => {
      const c = deck.pop()
      if (c) table.push(c)
      return c
    },
    draw: (_s: Seat, n: number) => {
      draws += n
    },
    insight: (_s: Seat, _c: Card, done: () => void) => {
      insights++
      done()
    },
    ask: (_s: Seat, _p: string, options: Option[]) => {
      actions.push(options)
    },
    log: () => {},
    emit: () => {},
    damage: (_s: Seat, n: number, _source: Seat | null, done: () => void) => {
      damage.push(n)
      done()
    },
    heal: (s: Seat) => {
      players[s].hp = Math.min(players[s].maxHp, players[s].hp + 1)
    },
    distance: () => 1,
    canAttack: () => true,
    requestSha: (_s: Seat, _p: string, cb: (yes: boolean) => void) => cb(false),
    requestShan: (_s: Seat, _p: string, _a: boolean, cb: (yes: boolean) => void) => cb(false),
  } as unknown as Game
  return {
    g,
    players,
    table,
    discard,
    deck,
    actions,
    damage,
    get draws() {
      return draws
    },
    get insights() {
      return insights
    },
  }
}

describe('trick resolution', () => {
  it('allows countering a counter and triggers insight for each response', () => {
    const f = fixture(),
      first = card('wuxie'),
      second = card('wuxie')
    f.players[0].hand.push(first)
    f.players[1].hand.push(second)
    let completed = false
    playTrick(f.g, 0, card('wuzhong'), () => {
      completed = true
    })
    f.actions.shift()![0].run()
    f.actions.shift()![0].run()
    expect(completed).toBe(true)
    expect(f.draws).toBe(2)
    expect(f.insights).toBe(2)
    expect(f.discard).toEqual([first, second])
  })
  it('keeps revealed harvest cards on the table until they are picked', () => {
    const f = fixture(),
      a = card('sha'),
      b = card('shan')
    f.deck.push(a, b)
    playTrick(f.g, 0, card('wugu'), () => {})
    expect(f.table).toHaveLength(2)
    f.actions.shift()![0].run()
    expect(f.table).toHaveLength(1)
    f.actions.shift()![0].run()
    expect(f.table).toHaveLength(0)
    expect(f.players.map((p) => p.hand.length)).toEqual([1, 1])
  })
  it('resolves delayed cards in reverse order and passes a missed lightning', () => {
    const f = fixture(),
      lebu = card('lebu'),
      lightning = card('shandian')
    f.players[0].judges.push(lebu, lightning)
    f.deck.push({ ...card('sha'), suit: 'club' }, { ...card('sha'), suit: 'heart' })
    let finished = false
    judgePhase(f.g, 0, () => {
      finished = true
    })
    expect(finished).toBe(true)
    expect(f.players[1].judges).toEqual([lightning])
    expect(f.g.skipPlay).toBe(true)
  })
  it('prevents duplicate delayed tricks and respects qicai distance', () => {
    const f = fixture()
    f.players[1].judges.push(card('lebu'))
    f.players[1].hand.push(card('sha'))
    expect(trickPlayable(f.g, 0, card('lebu'))).toBe(false)
    f.g.distance = () => 2
    expect(trickPlayable(f.g, 0, card('shunshou'))).toBe(false)
    f.players[0].hero = 'huangyueying'
    expect(trickPlayable(f.g, 0, card('shunshou'))).toBe(true)
  })
})

it('keeps counterspells on the real engine table through insight and the whole chain', async () => {
  const { Game } = await import('../src/engine')
  const g = new Game(1, 'zhangfei')
  g.deck.length = 0
  g.discard.length = 0
  g.table.length = 0
  for (const p of g.players) {
    p.hand.length = 0
    p.equipment.length = 0
    p.judges.length = 0
  }
  const original = card('wuzhong'),
    first = card('wuxie'),
    second = card('wuxie')
  g.table.push(original)
  g.players[0].hand.push(first)
  g.players[1].hand.push(second)
  let complete = false
  playTrick(g, 0, original, () => {
    complete = true
  })
  g.options[0].run()
  expect(g.table).toContain(first)
  g.options[0].run() // Yellow Yueying is asked about insight before continuing the chain.
  expect(g.table).toEqual([original, first, second])
  g.options[0].run() // Empty deck must not recycle either resolving counterspell.
  expect(complete).toBe(true)
  expect(g.players[1].hand).toEqual([])
  expect(g.table).toEqual([original])
  expect(g.players[0].hand).toHaveLength(2)
})
