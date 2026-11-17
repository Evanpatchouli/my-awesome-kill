import { definitions } from '@mak/rules'
import type { Card } from '@mak/rules'
import type { FeedbackEvent } from './battleMotion'
import roster from '../../../packages/duel-core/data/standard-2008-generals.json'
import deck from '../../../packages/duel-core/data/standard-deck.json'

export const generals = roster
export const generalById = new Map(roster.map((g) => [g.id, g]))
const skillTitles = new Map(
  roster.flatMap((hero) => hero.skills.map((skill) => [skill.id, skill.name] as const)),
)
export const skillName = (id: string) => skillTitles.get(id) ?? id
export const factions: Record<string, string> = { wei: '魏', shu: '蜀', wu: '吴', qun: '群' }
export const roles = { lord: '主公', loyalist: '忠臣', rebel: '反贼', renegade: '内奸' }
export const sides = { court: '主忠方', rebels: '反贼方', renegade: '内奸' }
export const phases: Record<string, string> = {
  preparation: '准备',
  judgment: '判定',
  draw: '摸牌',
  play: '出牌',
  discard: '弃牌',
  end: '结束',
}
export interface Entity {
  id: number
  kind: string
}
export interface Branch {
  id: number
  label: string
  cards: number[]
  targets: number[]
  minCards: number
  maxCards: number
  minTargets: number
  maxTargets: number
}
export type Action =
  | { type: 'endPlay' }
  | { type: 'playCard'; card: number; targets: number[] }
  | { type: 'choose'; event: number; card: number | null }
  | { type: 'discard'; event: number; cards: number[] }
  | { type: 'select'; event: number; branch: number; cards: number[]; targets: number[] }
  | { type: 'playVirtual'; kind: string; cards: number[]; targets: number[] }
  | { type: 'useSkill'; skill: string; cards: number[]; targets: number[] }
export interface SeatView {
  id: number
  hero: string
  role: keyof typeof roles | null
  alive: boolean
  hp: number
  maxHp: number
  handCount: number
  hand: Entity[] | null
  equipment: Entity[]
  judges: Entity[]
  judgeAssignments: Entity[]
}
export interface Choice {
  kind: 'shan' | 'save' | 'discard' | 'content'
  seat: number
  target: number | null
  event: number
  min: number
  max: number
  cards: number[]
  branches: Branch[]
  prompt?: string
  counter?: { cardKind: string; source: number | null; target: number; negated: boolean } | null
}
interface EventRecord {
  id: number
  type: string
  status: string
  parent: number | null
  seat?: number
  target?: number
  card?: Entity
  amount?: number
  before?: number
  after?: number
  kind?: string
  skill?: string
  role?: keyof typeof roles
}
export interface TableView {
  version: number
  activeSeat: number
  humanSeat: number
  waitingFor: number | null
  seats: SeatView[]
  turn: { round: number; number: number; phase: string; seat: number } | null
  counterWindow: boolean
  winner: keyof typeof sides | null
  drawn: boolean
  deckCount: number
  resolving: Entity[]
  discard: Entity[]
  choice: Choice | null
  actions: Action[]
  events: EventRecord[]
  virtualPlays: { kind: string; branch: Branch }[]
  skillPlays: { skill: string; branch: Branch }[]
  privateChoiceCards: Entity[]
}
export const cardName = (kind: string) => definitions[kind]?.name ?? kind
export function faceOf(card: Entity): Card {
  const d = deck[card.id]
  const def = definitions[card.kind]
  return {
    id: card.id,
    kind: card.kind,
    name: cardName(card.kind),
    suit: d.suit as Card['suit'],
    rank: d.rank,
    description: def?.description ?? '',
  }
}
export const cardById = (id: number) => faceOf(deck[id])
export function seatName(view: TableView, id: number | undefined) {
  const seat = view.seats.find((s) => s.id === id)
  return seat ? (generalById.get(seat.hero)?.name ?? seat.hero) : '牌堆'
}
const cardEffects: Partial<Record<string, 'play' | 'respond' | 'equip' | 'judge'>> = {
  useSha: 'play',
  useShaTargets: 'play',
  useTao: 'play',
  useTrick: 'play',
  equip: 'equip',
  useShan: 'respond',
  useResponseCard: 'respond',
  reveal: 'judge',
}

export function visualEvents(view: TableView): FeedbackEvent[] {
  const cues: FeedbackEvent[] = []
  const announced = new Set<number>()
  for (const e of view.events) {
    if (e.status !== 'started' || e.seat == null) continue
    if (e.type === 'changeHp' && e.before != null && e.after != null && e.before !== e.after) {
      cues.push({
        type: e.after < e.before ? 'damage' : 'heal',
        seat: e.seat,
        amount: Math.abs(e.after - e.before),
      })
    } else if (e.type === 'draw' && e.amount) {
      cues.push({ type: 'draw', seat: e.seat, amount: e.amount })
    } else if (e.card) {
      const type = e.type === 'useCost' && !announced.has(e.card.id) ? 'play' : cardEffects[e.type]
      if (type) {
        announced.add(e.card.id)
        cues.push({ type, seat: e.seat, cards: [faceOf(e.card)] })
      }
    }
  }
  return cues
}

export function counterPrompt(view: TableView): string | undefined {
  const info = view.choice?.counter
  if (!info) return
  const effect = `【${cardName(info.cardKind)}】对${seatName(view, info.target)}的效果`
  return info.negated
    ? `${effect}已被抵消，是否使用无懈可击恢复？`
    : `是否使用无懈可击抵消${effect}？`
}

export function describeEvent(view: TableView, e: EventRecord): string | undefined {
  if (e.status !== 'started') return
  const who = seatName(view, e.seat)
  if (e.card && cardEffects[e.type] && e.type !== 'reveal') {
    const verb = e.type === 'equip' ? '装备' : cardEffects[e.type] === 'respond' ? '打出' : '使用'
    return `${who}${verb}【${cardName(e.card.kind)}】${e.target == null ? '' : ' → ' + seatName(view, e.target)}`
  }
  if (e.type === 'useVirtual' && e.kind) return `${who}使用转化牌【${cardName(e.kind)}】`
  if (e.type === 'reveal' && e.card) return `${who}判定亮出【${cardName(e.card.kind)}】`
  if (e.type === 'changeHp') return `${who}体力 ${e.before} → ${e.after}`
  if (e.type === 'death') return `${seatName(view, e.target)}阵亡 · ${e.role ? roles[e.role] : ''}`
  if (e.type === 'draw') return `${who}摸了${e.amount}张牌`
  if (e.type === 'skill' && e.skill) return `${who}发动【${skillName(e.skill)}】`
}
