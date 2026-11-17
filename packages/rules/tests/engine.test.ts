import { createHash } from 'node:crypto'
import { describe, expect, it } from 'vitest'
import { standardDeck } from '../src/cards'
import { Game } from '../src/engine'
import { createMatch } from '../src'
import type { Hero, Seat } from '../src/model'

function setup(hero: Hero = 'zhangfei') {
  const g = new Game(817, hero)
  for (const p of g.players) g.deck.push(...p.hand.splice(0))
  const give = (s: Seat, kind: string, zone: 'hand' | 'equipment' | 'judges' = 'hand') => {
    const c = g.deck.find((c) => c.kind === kind)!
    g.move(c, zone, s)
    return c
  }
  const choose = (text: string) => {
    const index = g.options.findIndex((o) => o.label.includes(text))
    expect(index, `${g.prompt}: ${text}`).toBeGreaterThanOrEqual(0)
    expect(g.act(g.revision, g.key(index))).toBe(true)
  }
  return { g, give, choose }
}

describe('classic standard deck', () => {
  it('contains the exact number of physical cards and retains duplicate faces', () => {
    const deck = standardDeck()
    // Digest of the first 104 [suit, rank, kind] tuples in the source list (one slash-delimited face per line).
    const faces = deck.map((c) => [c.suit, c.rank, c.kind].join('/')).join('\n')
    expect(createHash('sha256').update(faces).digest('hex')).toBe(
      '51d115ccdf0672d4b57e88eb2c1769756012b79311ad7f33a05f21439f0f09a4',
    )
    expect(deck).toHaveLength(104)
    expect(new Set(deck.map((c) => c.id)).size).toBe(104)
    expect(new Set(deck.map((c) => c.kind)).size).toBe(30)
    for (const suit of ['spade', 'club', 'heart', 'diamond'])
      expect(deck.filter((c) => c.suit === suit)).toHaveLength(26)
    expect(deck.filter((c) => c.kind === 'sha' && c.rank === 8 && c.suit === 'spade')).toHaveLength(2)
    expect(deck.filter((c) => c.kind === 'wuxie')).toHaveLength(3)
    expect(deck.filter((c) => c.kind === 'shandian').map((c) => [c.suit, c.rank])).toEqual([
      ['spade', 1],
    ])
  })
})

describe('seat and command boundaries', () => {
  it('deals cards and rejects illegal or duplicate input without changing state', () => {
    const m = createMatch({ seed: 19, hero: 'zhangfei' })
    const before = m.view()
    expect(before.players.map((p) => p.handCount)).toEqual([6, 4])
    expect(before.players[1].hand).toEqual([])
    expect(m.act(before.revision, 'not an action')).toBe(false)
    expect(m.bot()).toBe(false)
    expect(m.view()).toEqual(before)
    const end = before.choices.find((o) => o.label === '结束出牌')!
    expect(m.act(before.revision, end.key)).toBe(true)
    const after = m.view()
    expect(after.phase).toBe('弃牌阶段')
    expect(m.act(before.revision, end.key)).toBe(false)
    expect(m.view()).toEqual(after)
  })
  it('discards exactly the required cards atomically and rejects invalid requests', () => {
    const m = createMatch({ seed: 19, hero: 'zhangfei' })
    const initial = m.view()
    expect(m.act(initial.revision, initial.choices.find((c) => c.label === '结束出牌')!.key)).toBe(true)
    const before = m.view()
    expect(before.discardCountRequired).toBe(2)
    const ids = before.players[0].hand.map((c) => c.id)
    for (const [revision, selected] of [[before.revision - 1, ids.slice(0, 2)], [before.revision, [ids[0]]], [before.revision, [ids[0], ids[0]]], [before.revision, [ids[0], -1]]] as [number, number[]][]) {
      expect(m.discard(revision, selected)).toBe(false)
      expect(m.view()).toEqual(before)
    }
    expect(m.discard(before.revision, ids.slice(0, 2))).toBe(true)
    const after = m.view()
    expect(after.events[0]).toMatchObject({ type: 'discard', seat: 0, cards: before.players[0].hand.slice(0, 2) })
    expect(m.discard(before.revision, ids.slice(0, 2))).toBe(false)
    expect(m.view()).toEqual(after)
  })
  it('emits public response and damage events for the successful command only', () => {
    const { g, give, choose } = setup()
    const sha = give(0, 'sha')
    const shan = give(1, 'shan')
    g.playPhase()
    choose('杀')
    expect(g.view().events).toEqual([{ type: 'play', seat: 0, cards: [sha] }])
    choose('出闪 ·')
    expect(g.view().events).toEqual([{ type: 'respond', seat: 1, cards: [shan] }])
    give(0, 'sha')
    g.playPhase()
    choose('杀')
    expect(g.view().events.map((event) => event.type)).toEqual(['play', 'damage'])
  })
  it.each(['zhangfei', 'huangyueying'] as const)(
    'finishes seeded matches with %s and conserves all cards after every decision',
    (hero) => {
      for (let seed = 1; seed <= 18; seed++) {
        const g = new Game(seed, hero)
        for (let i = 0; i < 1500 && g.winner === null; i++) {
          expect(g.best()).toBe(true)
          const cards = [
            ...g.deck,
            ...g.table,
            ...g.discard,
            ...g.players.flatMap((p) => [...p.hand, ...p.equipment, ...p.judges]),
          ]
          expect(cards.length).toBe(104)
          expect(new Set(cards.map((c) => c.id)).size).toBe(104)
        }
        expect(g.winner).not.toBeNull()
        const final = g.view()
        expect(g.act(g.revision, g.key(0))).toBe(false)
        expect(g.best()).toBe(false)
        expect(g.view()).toEqual(final)
        expect(g.table).toHaveLength(0)
      }
    },
  )
})

describe('standard combat', () => {
  it.each(['zhangfei', 'huangyueying'] as const)('enforces attack count with %s', (hero) => {
    const { g, give, choose } = setup(hero)
    give(0, 'sha')
    give(0, 'sha')
    g.playPhase()
    choose('使用')
    expect(g.players[1].hp).toBe(g.players[1].maxHp - 1)
    expect(g.options.some((o) => o.label.includes('杀'))).toBe(hero === 'zhangfei')
    give(0, 'zhuge', 'equipment')
    g.playPhase()
    expect(g.options.some((o) => o.label.includes('杀'))).toBe(true)
  })
  it('applies mount distance and weapon range before allowing an attack', () => {
    const { g, give } = setup()
    give(1, 'jueying', 'equipment')
    give(0, 'sha')
    g.playPhase()
    expect(g.options.map((o) => o.label)).toEqual(['结束出牌'])
    give(0, 'fangtian', 'equipment')
    g.playPhase()
    expect(g.canAttack(0)).toBe(true)
    expect(g.options.some((o) => o.label.includes('杀'))).toBe(true)
    give(0, 'chitu', 'equipment')
    expect(g.distance(0)).toBe(1)
  })
  it('bypasses armor with qinggang, but still permits a hand dodge', () => {
    const { g, give, choose } = setup()
    give(0, 'qinggang', 'equipment')
    give(1, 'bagua', 'equipment')
    give(1, 'shan')
    give(0, 'sha')
    g.playPhase()
    choose('使用')
    expect(g.actor).toBe(1)
    expect(g.options.some((o) => o.label.includes('八卦'))).toBe(false)
    choose('出闪 ·')
    expect(g.players[1].hp).toBe(3)
  })
  it.each([true, false])('resolves a red/black bagua judgment (%s)', (red) => {
    const { g, give, choose } = setup()
    give(1, 'bagua', 'equipment')
    give(1, 'shan')
    give(0, 'sha')
    const i = g.deck.findIndex((c) => c.suit === (red ? 'heart' : 'spade'))
    g.deck.push(...g.deck.splice(i, 1))
    g.playPhase()
    choose('使用')
    choose('八卦阵')
    if (!red) {
      expect(g.options.some((o) => o.label.includes('八卦'))).toBe(false)
      choose('出闪 ·')
    }
    expect(g.players[1].hp).toBe(3)
    expect(g.players[1].hand.length).toBe(red ? 1 : 0)
  })
  it('pays two distinct spear materials and holds them until the dodge window closes', () => {
    const { g, give, choose } = setup()
    give(0, 'zhangba', 'equipment')
    give(0, 'tao')
    give(0, 'shan')
    give(1, 'shan')
    g.playPhase()
    choose('丈八')
    choose('桃')
    choose('闪')
    expect(g.table.length).toBe(2)
    choose('出闪 ·')
    expect(g.table.length).toBe(0)
    expect(g.discard.length).toBe(3)
  })
  it('uses the axe after a dodge without consuming the axe', () => {
    const { g, give, choose } = setup()
    give(0, 'guanshi', 'equipment')
    give(0, 'sha')
    give(0, 'shan')
    give(0, 'tao')
    give(1, 'shan')
    g.playPhase()
    choose('杀')
    choose('出闪 ·')
    choose('贯石')
    choose('桃')
    choose('闪')
    expect(g.players[1].hp).toBe(2)
    expect(g.players[0].equipment[0].kind).toBe('guanshi')
  })
  it('uses the bow to remove a chosen mount', () => {
    const { g, give, choose } = setup()
    give(0, 'qilin', 'equipment')
    give(0, 'sha')
    give(1, 'dilu', 'equipment')
    give(1, 'chitu', 'equipment')
    g.playPhase()
    choose('杀')
    choose('的卢')
    expect(g.players[1].equipment.map((c) => c.kind)).toEqual(['chitu'])
    expect(g.players[1].hp).toBe(2)
  })
  it('offers the dual sword cost to the defender', () => {
    const { g, give, choose } = setup()
    give(0, 'cixiong', 'equipment')
    give(0, 'sha')
    give(1, 'tao')
    g.playPhase()
    choose('杀')
    choose('发动雌雄')
    expect(g.actor).toBe(1)
    choose('桃')
    expect(g.players[1].hand).toHaveLength(0)
  })
  it('permits follow-up attacks after a dodge and does not increment the phase quota', () => {
    const { g, give, choose } = setup('huangyueying')
    give(0, 'qinglong', 'equipment')
    give(0, 'sha')
    give(0, 'sha')
    give(1, 'shan')
    g.playPhase()
    choose('杀')
    choose('出闪 ·')
    choose('出杀 ·')
    expect(g.usedSha).toBe(1)
    expect(g.players[1].hp).toBe(3)
    expect(g.table).toHaveLength(0)
  })
  it('keeps asking for peaches until dying hp is positive', () => {
    const { g, give, choose } = setup()
    g.players[0].hp = 1
    give(0, 'tao')
    give(0, 'tao')
    give(1, 'tao')
    let resumed = false
    g.damage(0, 3, null, () => {
      resumed = true
    })
    choose('救援张飞')
    choose('救援张飞')
    expect(g.actor).toBe(1)
    expect(g.players[0].hp).toBe(0)
    choose('救援张飞')
    expect(resumed).toBe(true)
    expect(g.players[0].hp).toBe(1)
  })
})

describe('trick and skill integration', () => {
  it('offers insight for ordinary tricks before the effect, never for delayed tricks', () => {
    const { g, give, choose } = setup('huangyueying')
    give(0, 'lebu')
    give(0, 'wuzhong')
    g.playPhase()
    choose('乐不思蜀')
    expect(g.players[1].judges[0].kind).toBe('lebu')
    choose('无中生有')
    expect(g.prompt).toContain('集智')
    choose('集智')
    expect(g.players[0].hand.length).toBe(3)
  })
  it('alternates duel responses from the target until someone declines', () => {
    const { g, give, choose } = setup()
    give(0, 'juedou')
    give(0, 'sha')
    give(1, 'sha')
    g.playPhase()
    choose('决斗')
    expect(g.actor).toBe(1)
    choose('出杀 ·')
    expect(g.actor).toBe(0)
    choose('不出杀')
    expect(g.players[0].hp).toBe(3)
    expect(g.players[1].hp).toBe(3)
  })
  it('requires a weapon and legal attack range for collateral', () => {
    const { g, give } = setup()
    give(0, 'jiedao')
    give(0, 'dilu', 'equipment')
    g.playPhase()
    expect(g.options.some((o) => o.label.includes('借刀'))).toBe(false)
    const crossbow = give(1, 'zhuge', 'equipment')
    g.playPhase()
    expect(g.options.some((o) => o.label.includes('借刀'))).toBe(false)
    g.move(crossbow, 'discard')
    give(1, 'qinggang', 'equipment')
    g.playPhase()
    expect(g.options.some((o) => o.label.includes('借刀'))).toBe(true)
  })
  it.each([true, false])('resolves collateral on the user or yields the weapon (%s)', (attack) => {
    const { g, give, choose } = setup()
    give(0, 'jiedao')
    give(1, 'qinggang', 'equipment')
    give(1, 'sha')
    g.playPhase()
    choose('借刀')
    expect(g.actor).toBe(1)
    choose(attack ? '出杀 ·' : '不出杀')
    expect(g.players[0].hp).toBe(attack ? 3 : 4)
    expect(g.players[0].hand.some((c) => c.kind === 'qinggang')).toBe(!attack)
  })
  it('picks a hidden hand without disclosing faces and can dismantle a public delayed trick', () => {
    const { g, give, choose } = setup()
    give(0, 'shunshou')
    give(0, 'guohe')
    give(1, 'tao')
    give(1, 'lebu', 'judges')
    g.playPhase()
    choose('顺手')
    expect(
      g
        .view()
        .choices.map((o) => o.label)
        .join(),
    ).not.toContain('桃')
    choose('随机')
    expect(g.players[0].hand.some((c) => c.kind === 'tao')).toBe(true)
    choose('过河')
    choose('乐不思蜀')
    expect(g.players[1].judges).toHaveLength(0)
    expect(g.discard.some((c) => c.kind === 'lebu')).toBe(true)
  })
  it('settles global recovery once per target and caps hp', () => {
    const { g, give, choose } = setup()
    give(0, 'taoyuan')
    g.players[0].hp = 2
    g.playPhase()
    choose('桃园')
    expect(g.players.map((p) => p.hp)).toEqual([3, 3])
  })
  it.each([
    ['nanman', '南蛮', 'sha'],
    ['wanjian', '万箭', 'shan'],
  ])('requires the correct response to %s', (kind, name, response) => {
    const { g, give, choose } = setup()
    give(0, kind)
    const c = give(1, response)
    give(1, 'tao')
    g.playPhase()
    choose(name)
    expect(g.options.filter((o) => o.card !== undefined).map((o) => o.card)).toEqual([c.id])
    choose(response === 'sha' ? '不出杀' : '不出闪')
    expect(g.players[1].hp).toBe(2)
  })
})

it('reveals both harvest cards before the first counter and removes each after transfer', () => {
  const { g, give, choose } = setup()
  give(0, 'wugu')
  give(0, 'wuxie')
  g.playPhase()
  choose('五谷丰登')
  expect(g.prompt).toContain('无懈')
  const exposed = g.view().revealed
  expect(exposed).toHaveLength(2)
  choose('不使用无懈')
  choose('获得')
  expect(g.view().revealed).toHaveLength(1)
  choose('不使用无懈')
  choose('获得')
  expect(g.view().revealed).toEqual([])
  expect([...g.players[0].hand, ...g.players[1].hand].map((c) => c.id)).toEqual(
    expect.arrayContaining(exposed.map((c) => c.id)),
  )
})
