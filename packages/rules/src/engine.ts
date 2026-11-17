import { definitions, face, slot, standardDeck, type Card } from './cards'
import { attack, attackWith, requestAttack, requestSha, requestShan, spear } from './combat'
import { judgePhase, playTrick, trickPlayable } from './tricks'
import { keepValue, player, type Done, type Hero, type Option, type Player, type Seat } from './model'

type Zone = 'hand' | 'equipment' | 'judges' | 'discard' | 'table'

export class Game {
  readonly players: [Player, Player]
  readonly deck = standardDeck()
  readonly table: Card[] = []
  readonly revealed = new Set<Card>()
  readonly discard: Card[] = []
  readonly messages: string[] = []
  turn = 0
  active: Seat = 0
  actor: Seat = 0
  winner: Seat | null = null
  revision = 0
  phase = ''
  prompt = ''
  options: Option[] = []
  skipPlay = false
  usedSha = 0
  private randomState: number

  constructor(seed: number, hero: Hero) {
    this.randomState = seed >>> 0
    this.players = [player(hero), player(hero === 'zhangfei' ? 'huangyueying' : 'zhangfei')]
    this.shuffle(this.deck)
    this.draw(0, 4)
    this.draw(1, 4)
    this.startTurn(0)
  }

  other(s: Seat): Seat {
    return s === 0 ? 1 : 0
  }
  private randomIndex(n: number) {
    this.randomState = (Math.imul(this.randomState, 1664525) + 1013904223) >>> 0
    return Math.floor((this.randomState / 4294967296) * n)
  }
  private shuffle(cards: Card[]) {
    for (let i = cards.length - 1; i > 0; i--) {
      const j = this.randomIndex(i + 1)
      ;[cards[i], cards[j]] = [cards[j], cards[i]]
    }
  }
  log(message: string) {
    this.messages.push(message)
    if (this.messages.length > 80) this.messages.shift()
  }
  reveal(): Card | undefined {
    if (!this.deck.length && this.discard.length) {
      this.deck.push(...this.discard.splice(0))
      this.shuffle(this.deck)
      this.log('弃牌堆已洗回摸牌堆')
    }
    const c = this.deck.pop()
    if (c) this.table.push(c)
    return c
  }
  move(c: Card, zone: Zone, s?: Seat) {
    const piles = [
      this.table,
      this.discard,
      this.deck,
      ...this.players.flatMap((p) => [p.hand, p.equipment, p.judges]),
    ]
    const from = piles.find((pile) => pile.includes(c))
    if (!from) throw new Error('牌缺少来源区域')
    const to = zone === 'discard' ? this.discard : zone === 'table' ? this.table : this.players[s!][zone]
    if (from === this.table) this.revealed.delete(c)
    from.splice(from.indexOf(c), 1)
    to.push(c)
  }
  draw(s: Seat, count: number) {
    let n = 0
    for (; n < count; n++) {
      const c = this.reveal()
      if (!c) break
      this.move(c, 'hand', s)
    }
    this.log(`${this.players[s].name}摸了${n}张牌`)
  }
  ask(s: Seat, prompt: string, options: Option[]) {
    if (this.winner !== null) return
    if (!options.length) throw new Error('空的决策列表')
    this.actor = s
    this.prompt = prompt
    this.options = options
  }
  key(index: number) {
    return `${this.revision}/${index}`
  }
  act(revision: number, key: string) {
    if (this.winner !== null || revision !== this.revision) return false
    const idx = this.options.findIndex((_, i) => this.key(i) === key)
    if (idx < 0) return false
    const option = this.options[idx]
    this.revision++
    this.options = []
    option.run()
    return true
  }
  best() {
    if (!this.options.length) return false
    const best = this.options.reduce((a, b) => (b.score > a.score ? b : a))
    return this.act(this.revision, this.key(this.options.indexOf(best)))
  }
  view() {
    const playerView = (p: Player, hidden: boolean) => ({
      name: p.name,
      hp: p.hp,
      maxHp: p.maxHp,
      handCount: p.hand.length,
      hand: hidden ? [] : p.hand.map((c) => ({ ...c })),
      equipment: p.equipment.map((c) => ({ ...c })),
      judges: p.judges.map((c) => ({ ...c })),
    })
    return {
      revision: this.revision,
      turn: this.turn,
      active: this.active,
      actor: this.actor,
      phase: this.phase,
      prompt: this.actor === 1 && this.winner === null ? '电脑正在思考' : this.prompt,
      revealed: [...this.revealed].map((c) => ({ ...c })),
      winner: this.winner,
      players: [playerView(this.players[0], false), playerView(this.players[1], true)] as const,
      choices:
        this.actor === 0
          ? this.options.map(({ label, card }, i) => ({ label, card, key: this.key(i) }))
          : [],
      log: [...this.messages],
      drawCount: this.deck.length,
      discardCount: this.discard.length,
    }
  }
  heal(s: Seat) {
    const p = this.players[s]
    if (p.hp < p.maxHp) {
      p.hp++
      this.log(`${p.name}回复一点体力`)
    }
  }
  damage(s: Seat, amount: number, source: Seat | null, done: Done) {
    const p = this.players[s]
    p.hp -= amount
    this.log(`${p.name}受到${source === null ? '闪电' : this.players[source].name}造成的${amount}点伤害`)
    if (p.hp > 0) {
      done()
      return
    }
    const rescue = (seat: Seat, passes: number) => {
      if (p.hp > 0) {
        done()
        return
      }
      if (passes === 2) {
        for (const card of [...this.table]) this.move(card, 'discard')
        this.winner = this.other(s)
        this.phase = '终局'
        this.prompt = `${this.players[this.winner].name}获胜`
        this.options = []
        this.log(this.prompt)
        return
      }
      const cards = this.players[seat].hand.filter((c) => c.kind === 'tao')
      const pass = () => rescue(this.other(seat), passes + 1)
      if (!cards.length) {
        pass()
        return
      }
      this.ask(seat, `${p.name}濒死（体力 ${p.hp}），是否使用桃？`, [
        ...cards.map((c) => ({
          label: `使用${face(c)}救援${p.name}`,
          card: c.id,
          score: seat === s ? 20 : -10,
          run: () => {
            this.move(c, 'discard')
            this.heal(s)
            rescue(seat, passes)
          },
        })),
        { label: '放弃救援', score: 0, run: pass },
      ])
    }
    rescue(this.active, 0)
  }
  distance(s: Seat) {
    const horse = (who: Seat, type: string) =>
      Number(this.players[who].equipment.some((c) => slot(c.kind) === type))
    return Math.max(1, 1 + horse(this.other(s), 'plus') - horse(s, 'minus'))
  }
  canAttack(s: Seat) {
    const weapon = this.players[s].equipment.find((c) => slot(c.kind) === 'weapon')
    return this.distance(s) <= (weapon ? definitions[weapon.kind].range! : 1)
  }
  attack(s: Seat, t: Seat, done: Done) {
    attack(this, s, t, done)
  }
  requestAttack(s: Seat, t: Seat, prompt: string, done: Done, decline: Done) {
    requestAttack(this, s, t, prompt, done, decline)
  }
  requestSha(s: Seat, prompt: string, cb: (ok: boolean) => void, score?: number) {
    requestSha(this, s, prompt, cb, score)
  }
  requestShan(s: Seat, prompt: string, ignore: boolean, cb: (ok: boolean) => void) {
    requestShan(this, s, prompt, ignore, cb)
  }

  pickZone(
    chooser: Seat,
    owner: Seat,
    prompt: string,
    picked: (c: Card) => void,
    includeJudges: boolean,
  ) {
    const p = this.players[owner]
    const options: Option[] = p.equipment.map((c) => ({
      label: face(c),
      score: 8,
      run: () => picked(c),
    }))
    if (includeJudges)
      options.push(...p.judges.map((c) => ({ label: face(c), score: -3, run: () => picked(c) })))
    if (p.hand.length)
      options.push({
        label: '随机选一张手牌',
        score: 5,
        run: () => picked(p.hand[this.randomIndex(p.hand.length)]),
      })
    this.ask(chooser, prompt, options)
  }
  insight(s: Seat, c: Card, done: Done) {
    if (
      this.players[s].hero !== 'huangyueying' ||
      slot(c.kind) ||
      ['sha', 'shan', 'tao', 'lebu', 'shandian'].includes(c.kind)
    ) {
      done()
      return
    }
    this.ask(s, `使用${c.name} · 是否发动集智？`, [
      {
        label: '集智 · 摸一张牌',
        score: 9,
        run: () => {
          this.draw(s, 1)
          done()
        },
      },
      { label: '放弃发动', score: 0, run: done },
    ])
  }

  private startTurn(s: Seat) {
    this.turn++
    this.active = s
    this.skipPlay = false
    this.usedSha = 0
    this.phase = '判定阶段'
    this.log(`第${this.turn}回合 · ${this.players[s].name}`)
    judgePhase(this, s, () => {
      this.draw(s, 2)
      if (this.skipPlay) {
        this.log('乐不思蜀跳过出牌阶段')
        this.drop(s)
      } else this.playPhase()
    })
  }

  playPhase() {
    const s = this.active,
      t = this.other(s)
    const p = this.players[s],
      target = this.players[t]
    const canSha =
      this.canAttack(s) &&
      (!this.usedSha || p.hero === 'zhangfei' || p.equipment.some((c) => c.kind === 'zhuge'))
    this.phase = '出牌阶段'
    const options: Option[] = []
    for (const c of p.hand) {
      const equip = slot(c.kind)
      const legal =
        equip ||
        (c.kind === 'sha' && canSha) ||
        (c.kind === 'tao' && p.hp < p.maxHp) ||
        trickPlayable(this, s, c)
      if (!legal) continue
      const rival = [
        'sha',
        'lebu',
        'jiedao',
        'shunshou',
        'guohe',
        'juedou',
        'wanjian',
        'nanman',
      ].includes(c.kind)
      let score = rival ? 7 : 5
      if (equip) score = p.equipment.some((x) => slot(x.kind) === equip) ? -2 : 10
      if (c.kind === 'taoyuan' && (p.hp >= p.maxHp || target.hp < target.maxHp)) score = -1
      if (c.kind === 'tao') score = 12
      if (c.kind === 'shandian') score = -1
      options.push({
        label: `${equip ? '装备' : '使用'}${face(c)}${rival ? ' → ' + target.name : ''}`,
        card: c.id,
        score,
        run: () => {
          this.move(c, 'table')
          this.log(`${p.name}使用${face(c)}${rival ? ' → ' + target.name : ''}`)
          const finish = () => {
            if (this.table.includes(c)) this.move(c, 'discard')
            this.playPhase()
          }
          if (equip) {
            const old = p.equipment.find((x) => slot(x.kind) === equip)
            if (old) this.move(old, 'discard')
            this.move(c, 'equipment', s)
            finish()
          } else if (c.kind === 'sha') {
            this.usedSha++
            this.attack(s, t, finish)
          } else if (c.kind === 'tao') {
            this.heal(s)
            finish()
          } else this.insight(s, c, () => playTrick(this, s, c, finish))
        },
      })
    }
    if (canSha && p.hand.length >= 2 && p.equipment.some((c) => c.kind === 'zhangba')) {
      options.push({
        label: `丈八蛇矛 · 两张手牌当杀 → ${target.name}`,
        score: 1,
        run: () => {
          this.usedSha++
          spear(this, s, (cards) => attackWith(this, s, t, cards, () => this.playPhase()))
        },
      })
    }
    options.push({ label: '结束出牌', score: 0, run: () => this.drop(s) })
    this.ask(s, `${p.name}的出牌阶段 · 选择一张牌或结束出牌`, options)
  }
  private drop(s: Seat) {
    this.phase = '弃牌阶段'
    const p = this.players[s],
      excess = p.hand.length - p.hp
    if (excess <= 0) {
      this.startTurn(this.other(s))
      return
    }
    this.ask(
      s,
      `还须弃置${excess}张手牌`,
      p.hand.map((c) => ({
        label: `弃${face(c)}`,
        score: -keepValue(c),
        card: c.id,
        run: () => {
          this.move(c, 'discard')
          this.log(`${p.name}弃置${face(c)}`)
          this.drop(s)
        },
      })),
    )
  }
}
