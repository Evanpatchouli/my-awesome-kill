import { heroes } from './heroes'
import { face, slot } from './cards'
import { keepValue, type Card, type Done, type Game, type Option, type Seat } from './model'

function holds(g: Game, s: Seat, kind: string) {
  return g.players[s].equipment.some((c) => c.kind === kind)
}

function payPair(g: Game, s: Seat, cards: Card[], name: string, paid: (cards: Card[]) => void) {
  const choose = (available: Card[], first?: Card) => {
    g.ask(
      s,
      first ? `${name} · 选择第二张牌（已选 ${face(first)}）` : `${name} · 选择第一张牌`,
      available.map((c) => ({
        label: face(c),
        card: c.id,
        score: -keepValue(c),
        run: () => {
          if (!first) {
            choose(
              available.filter((other) => other !== c),
              c,
            )
            return
          }
          g.log(`${g.players[s].name}发动${name}，选用${face(first)}、${face(c)}`)
          paid([first, c])
        },
      })),
    )
  }
  choose(cards)
}

export function spear(g: Game, s: Seat, paid: (cards: Card[]) => void) {
  payPair(g, s, [...g.players[s].hand], '丈八蛇矛', paid)
}

function offerSha(
  g: Game,
  s: Seat,
  prompt: string,
  paid: (cards: Card[]) => void,
  decline: Done,
  score: number,
) {
  const p = g.players[s]
  const options: Option[] = p.hand
    .filter((c) => c.kind === 'sha')
    .map((c) => ({
      label: `出杀 · ${face(c)}`,
      card: c.id,
      score,
      run: () => {
        g.log(`${p.name}打出${face(c)}`)
        paid([c])
      },
    }))
  if (holds(g, s, 'zhangba') && p.hand.length >= 2)
    options.push({ label: '丈八蛇矛 · 两张手牌当杀', score: score - 2, run: () => spear(g, s, paid) })
  if (!options.length) {
    decline()
    return
  }
  options.push({ label: '不出杀', score: 0, run: decline })
  g.ask(s, prompt, options)
}

export function requestSha(g: Game, s: Seat, prompt: string, result: (ok: boolean) => void, score = 10) {
  offerSha(
    g,
    s,
    prompt,
    (cards) => {
      cards.forEach((c) => g.move(c, 'discard'))
      g.emit({ type: 'respond', seat: s, cards })
      result(true)
    },
    () => result(false),
    score,
  )
}

export function attackWith(g: Game, s: Seat, target: Seat, cards: Card[], done: Done) {
  cards.forEach((c) => g.move(c, 'table'))
  g.emit({ type: 'play', seat: s, cards })
  attack(g, s, target, () => {
    cards.forEach((c) => g.move(c, 'discard'))
    done()
  })
}

export function requestAttack(g: Game, s: Seat, t: Seat, prompt: string, done: Done, decline: Done) {
  offerSha(g, s, prompt, (cards) => attackWith(g, s, t, cards, done), decline, 10)
}

export function requestShan(
  g: Game,
  s: Seat,
  prompt: string,
  ignoreArmor: boolean,
  result: (ok: boolean) => void,
) {
  const p = g.players[s]
  const respond = (allowArmor: boolean) => {
    const choices: Option[] = p.hand
      .filter((c) => c.kind === 'shan')
      .map((c) => ({
        label: `出闪 · ${face(c)}`,
        score: 9,
        card: c.id,
        run: () => {
          g.move(c, 'discard')
          g.emit({ type: 'respond', seat: s, cards: [c] })
          g.log(`${p.name}打出${face(c)}`)
          result(true)
        },
      }))
    if (allowArmor && holds(g, s, 'bagua'))
      choices.push({
        label: '八卦阵 · 判定',
        score: 12,
        run: () => {
          const c = g.reveal()
          if (c) {
            g.emit({ type: 'judge', seat: s, cards: [c] })
            g.log(`八卦阵判定：${face(c)}`)
            g.move(c, 'discard')
          }
          if (c?.suit === 'heart' || c?.suit === 'diamond') result(true)
          else respond(false)
        },
      })
    if (!choices.length) {
      result(false)
      return
    }
    choices.push({ label: '不出闪', score: 0, run: () => result(false) })
    g.ask(s, prompt, choices)
  }
  respond(!ignoreArmor)
}

export function attack(g: Game, s: Seat, t: Seat, done: Done) {
  const p = g.players[s],
    target = g.players[t]
  const hit = () => {
    const hurt = () => g.damage(t, 1, s, done)
    const mounts = target.equipment.filter((c) => ['plus', 'minus'].includes(slot(c.kind) ?? ''))
    if (!holds(g, s, 'qilin') || !mounts.length) {
      hurt()
      return
    }
    g.ask(s, '麒麟弓 · 可弃置对方一匹坐骑', [
      ...mounts.map((c) => ({
        label: `弃置${face(c)}`,
        score: 8,
        run: () => {
          g.move(c, 'discard')
          g.emit({ type: 'discard', seat: t, cards: [c] })
          g.log(`麒麟弓弃置${c.name}`)
          hurt()
        },
      })),
      { label: '保留坐骑', score: 0, run: hurt },
    ])
  }
  const dodge = () =>
    requestShan(g, t, `${p.name}对你使用杀`, holds(g, s, 'qinggang'), (ok) => {
      if (!ok) {
        hit()
        return
      }
      g.log(`${target.name}以闪抵消杀`)
      if (holds(g, s, 'qinglong')) {
        requestAttack(g, s, t, '青龙偃月刀 · 可继续对同一目标使用杀', done, done)
        return
      }
      const costs = [...p.hand, ...p.equipment.filter((c) => c.kind !== 'guanshi')]
      if (holds(g, s, 'guanshi') && costs.length >= 2) {
        g.ask(s, '贯石斧 · 可弃两张牌强制命中', [
          {
            label: '贯石斧 · 弃两张牌',
            score: target.hp <= 1 ? 10 : 1,
            run: () =>
              payPair(g, s, costs, '贯石斧', (cards) => {
                cards.forEach((c) => g.move(c, 'discard'))
                g.emit({ type: 'discard', seat: s, cards })
                hit()
              }),
          },
          { label: '放弃发动', score: 0, run: done },
        ])
      } else done()
    })
  if (!holds(g, s, 'cixiong') || heroes[p.hero].sex === heroes[target.hero].sex) {
    dodge()
    return
  }
  const gain = () => {
    g.draw(s, 1)
    dodge()
  }
  const offer = () => {
    if (!target.hand.length) {
      gain()
      return
    }
    g.ask(t, '雌雄双股剑 · 弃一张手牌，或令对方摸一张', [
      ...target.hand.map((c) => ({
        label: `弃${face(c)}`,
        card: c.id,
        score: 5 - keepValue(c),
        run: () => {
          g.move(c, 'discard')
          g.emit({ type: 'discard', seat: t, cards: [c] })
          g.log(`${target.name}弃置${face(c)}`)
          dodge()
        },
      })),
      { label: '令对方摸一张牌', score: 0, run: gain },
    ])
  }
  g.ask(s, '雌雄双股剑 · 对异性角色使用杀', [
    { label: '发动雌雄双股剑', score: 8, run: offer },
    { label: '放弃发动', score: 0, run: dodge },
  ])
}
