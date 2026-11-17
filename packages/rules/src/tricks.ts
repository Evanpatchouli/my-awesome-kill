import type { Seat, Card, Game, Option, Done } from './model'
import { face, slot } from './cards'

const hasCards = (g: Game, s: Seat) =>
  g.players[s].hand.length + g.players[s].equipment.length + g.players[s].judges.length > 0
const hasJudge = (g: Game, s: Seat, kind: string) => g.players[s].judges.some((c) => c.kind === kind)
const isQicai = (g: Game, s: Seat) => g.players[s].hero === 'huangyueying'

/** Whether a trick can be used in the current two-player position. */
export function trickPlayable(g: Game, s: Seat, c: Card): boolean {
  const t = g.other(s)
  switch (c.kind) {
    case 'wugu':
    case 'taoyuan':
    case 'nanman':
    case 'wanjian':
    case 'juedou':
    case 'wuzhong':
      return true
    case 'shunshou':
      return hasCards(g, t) && (isQicai(g, s) || g.distance(s) <= 1)
    case 'guohe':
      return hasCards(g, t)
    case 'jiedao':
      return g.players[t].equipment.some((card) => slot(card.kind) === 'weapon') && g.canAttack(t)
    case 'lebu':
      return !hasJudge(g, t, 'lebu')
    case 'shandian':
      return !hasJudge(g, s, 'shandian')
    default:
      return false
  }
}

// Each response flips the effect. Two consecutive passes close the chain.
function counter(
  g: Game,
  target: Seat,
  trick: Card,
  beneficial: boolean,
  done: (effective: boolean) => void,
): void {
  let effective = true
  let passes = 0
  const responses: Card[] = []
  const next = (s: Seat): void => {
    const cards = g.players[s].hand.filter((c) => c.kind === 'wuxie')
    if (!cards.length) {
      pass(s)
      return
    }
    const wantsCancel = (s === target ? !beneficial : beneficial) === effective
    const options: Option[] = cards.map((card) => ({
      label: `使用无懈可击（${card.name}）`,
      card: card.id,
      score: wantsCancel ? 80 : -10,
      run: () => {
        passes = 0
        effective = !effective
        g.move(card, 'table')
        responses.push(card)
        g.log(`${g.players[s].name}使用了无懈可击`)
        g.insight(s, card, () => next(g.other(s)))
      },
    }))
    options.push({ label: '不使用无懈可击', score: 0, run: () => pass(s) })
    g.ask(
      s,
      trick.name +
        '对' +
        g.players[target].name +
        '的效果' +
        (effective ? '' : '已被无懈') +
        '，是否' +
        (effective ? '使用无懈可击' : '反无懈') +
        '？',
      options,
    )
  }
  const pass = (s: Seat): void => {
    if (++passes === 2) {
      responses.forEach((card) => g.move(card, 'discard'))
      done(effective)
    } else next(g.other(s))
  }
  next(g.active)
}

function targets(
  g: Game,
  list: Seat[],
  trick: Card,
  beneficial: boolean,
  effect: (s: Seat, next: Done) => void,
  done: Done,
): void {
  const step = (i: number): void => {
    if (i === list.length) {
      done()
      return
    }
    const s = list[i]
    counter(g, s, trick, beneficial, (effective) =>
      effective ? effect(s, () => step(i + 1)) : step(i + 1),
    )
  }
  step(0)
}

export function playTrick(g: Game, s: Seat, c: Card, done: Done): void {
  const t = g.other(s)
  const one = (target: Seat, beneficial: boolean, effect: (next: Done) => void) =>
    targets(g, [target], c, beneficial, (_target, next) => effect(next), done)
  switch (c.kind) {
    case 'wuzhong':
      one(s, true, (next) => {
        g.draw(s, 2)
        next()
      })
      return
    case 'taoyuan':
      targets(
        g,
        [s, t],
        c,
        true,
        (target, next) => {
          g.heal(target)
          next()
        },
        done,
      )
      return
    case 'wugu': {
      const revealed: Card[] = []
      for (let i = 0; i < 2; i++) {
        const card = g.reveal()
        if (card) revealed.push(card)
      }
      revealed.forEach((card) => g.revealed.add(card))
      g.log(`五谷丰登亮出：${revealed.map(face).join('、')}`)
      targets(
        g,
        [s, t],
        c,
        true,
        (target, next) => {
          if (!revealed.length) {
            next()
            return
          }
          g.ask(
            target,
            '选择一张五谷丰登的牌',
            revealed.map((card) => ({
              label: `获得${face(card)}`,
              card: card.id,
              score: 1,
              run: () => {
                revealed.splice(revealed.indexOf(card), 1)
                g.move(card, 'hand', target)
                g.log(g.players[target].name + '获得了' + face(card))
                next()
              },
            })),
          )
        },
        () => {
          revealed.forEach((card) => g.move(card, 'discard'))
          done()
        },
      )
      return
    }
    case 'nanman':
      one(t, false, (next) =>
        g.requestSha(t, '南蛮入侵：请使用一张杀', (success) =>
          success ? next() : g.damage(t, 1, s, next),
        ),
      )
      return
    case 'wanjian':
      one(t, false, (next) =>
        g.requestShan(t, '万箭齐发：请使用一张闪', false, (success) =>
          success ? next() : g.damage(t, 1, s, next),
        ),
      )
      return
    case 'juedou':
      one(t, false, (next) => {
        const respond = (who: Seat): void =>
          g.requestSha(who, '决斗：请使用一张杀', (success) => {
            if (success) respond(g.other(who))
            else g.damage(who, 1, g.other(who), next)
          })
        respond(t)
      })
      return
    case 'shunshou':
    case 'guohe':
      one(t, false, (next) => {
        if (!hasCards(g, t)) {
          next()
          return
        }
        g.pickZone(
          s,
          t,
          c.kind === 'shunshou' ? '选择获得的牌' : '选择弃置的牌',
          (card) => {
            if (c.kind === 'shunshou') g.move(card, 'hand', s)
            else g.move(card, 'discard')
            next()
          },
          true,
        )
      })
      return
    case 'jiedao':
      one(t, false, (next) => {
        g.requestAttack(t, s, '借刀杀人：对对方使用杀，或交出武器', next, () => {
          const weapon = g.players[t].equipment.find((card) => slot(card.kind) === 'weapon')
          if (weapon) g.move(weapon, 'hand', s)
          next()
        })
      })
      return
    case 'lebu':
      g.move(c, 'judges', t)
      done()
      return
    case 'shandian':
      g.move(c, 'judges', s)
      done()
      return
    default:
      done()
  }
}

/** Resolve delayed tricks last-in-first-out, including their counterspell windows. */
export function judgePhase(g: Game, s: Seat, done: Done): void {
  const step = (): void => {
    const delayed = g.players[s].judges[g.players[s].judges.length - 1]
    if (!delayed) {
      done()
      return
    }
    g.move(delayed, 'table')
    const finishLightning = (): void => {
      const t = g.other(s)
      if (hasJudge(g, t, 'shandian')) g.move(delayed, 'discard')
      else g.move(delayed, 'judges', t)
      step()
    }
    counter(g, s, delayed, false, (effective) => {
      if (!effective) {
        if (delayed.kind === 'shandian') finishLightning()
        else {
          g.move(delayed, 'discard')
          step()
        }
        return
      }
      const result = g.reveal()
      if (result) {
        g.log(g.players[s].name + '的' + delayed.name + '判定为' + face(result))
        g.move(result, 'discard')
      } else g.log(g.players[s].name + '的' + delayed.name + '没有判定牌')
      if (delayed.kind === 'lebu') {
        if (!result || result.suit !== 'heart') {
          g.skipPlay = true
          g.log('乐不思蜀判定失败，跳过出牌阶段')
        } else g.log('乐不思蜀判定成功')
        g.move(delayed, 'discard')
        step()
      } else if (delayed.kind === 'shandian') {
        if (result && result.suit === 'spade' && result.rank >= 2 && result.rank <= 9) {
          g.log('闪电判定命中')
          g.move(delayed, 'discard')
          g.damage(s, 3, null, step)
        } else {
          g.log('闪电判定未命中')
          finishLightning()
        }
      } else {
        g.move(delayed, 'discard')
        step()
      }
    })
  }
  step()
}
