import { definitions } from '@mak/rules'
import type { Card } from '@mak/rules'
import type { FeedbackEvent } from './battleMotion'
import roster from '../../../packages/duel-core/data/standard-2008-generals.json'
import deck from '../../../packages/duel-core/data/standard-deck.json'

export const generals = roster
export type GeneralDefinition = typeof generals[number]
export const generalById = new Map(roster.map((g) => [g.id, g]))
export interface ContentPreview {
  generals: GeneralDefinition[]
  cards: { id: string; name: string; description: string }[]
}
export function generalDefinition(view: TableView, id: string) {
  return generalById.get(id) ?? view.content?.generals.find((general) => general.id === id)
}
const skillTitles = new Map(
  roster.flatMap((hero) => hero.skills.map((skill) => [skill.id, skill.name] as const)),
)
const extraSkillTitles: Record<string, string> = {
  zhongyi: '忠义', zhongyi2: '忠义', dangxian: '当先', fangquan: '放权', fangquan2: '放权',
  gushe: '鼓舌', twdanlie: '胆烈', clanmingdian: '明典', mbdanggu: '党锢', mbmowang: '殁亡',
}
export const skillName = (id: string) => extraSkillTitles[id] ?? skillTitles.get(id) ?? id
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
interface CompareParticipants {
  initiator: number
  target: number
}
interface CompareGroup {
  initiator: number
  targets: number[]
}
type CompareMode = 'pairwise' | 'simultaneous'
export type GroupAnnouncement =
  | { type: 'reveal'; comparison: number; mode: CompareMode; bids: { seat: number; card: Entity }[] }
  | { type: 'pair'; comparison: number; initiator: number; target: number; numbers: [number, number]; winner: number | null }
  | { type: 'result'; comparison: number; mode: CompareMode; group: CompareGroup; numbers: [number, number][]; winner: number | null }
interface CompareResult {
  event: number
  parent: number | null
  turn: number | null
  participants: CompareParticipants
  cards: [Entity, Entity]
  numbers: [number, number]
  won: boolean
  tie: boolean
  winner: number | null
}
export type DamageNature = 'fire' | 'thunder'
export type TableCard = Omit<Card, 'suit' | 'rank'> & { suit?: Card['suit']; rank?: number }
const damageNames: Record<DamageNature, string> = { fire: '火焰', thunder: '雷电' }
const tiesuo = {
  name: '铁索连环',
  description: '出牌阶段，对一至两名角色使用，分别横置或重置其武将牌。此牌可重铸。',
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
export interface MoveStep {
  card: number
  row: number
  index: number
}
export interface MoveChoiceView {
  prompt: string
  rows: { label: string; cards: number[]; min: number; max: number }[]
  allowDecline: boolean
}
export type Action =
  | { type: 'endPlay' | 'advanceRound' }
  | { type: 'playCard'; card: number; targets: number[] }
  | { type: 'recast'; card: number }
  | { type: 'choose'; event: number; card: number | null }
  | { type: 'discard'; event: number; cards: number[] }
  | { type: 'select'; event: number; branch: number; cards: number[]; targets: number[] }
  | { type: 'moveCards'; event: number; moves: MoveStep[] | null }
  | { type: 'selectEquipmentButtons'; event: number; buttons: number[] }
  | { type: 'playVirtual'; kind: string; cards: number[]; targets: number[] }
  | { type: 'useSkill'; skill: string; cards: number[]; targets: number[] }
  | { type: 'activateSkill'; registration: number; cards: number[]; targets: number[] }
export interface SeatView {
  id: number
  hero: string
  role: keyof typeof roles | null
  alive: boolean
  out?: boolean
  rest?: { unit: 'phase' | 'round'; count: number }
  judgeDisabled?: boolean
  judgeDisabledMarker?: boolean
  linked?: boolean
  turnedOver?: boolean
  armor?: number
  hp: number
  maxHp: number
  handCount: number
  hand: Entity[] | null
  knownHand?: Entity[]
  expansions?: { card: Entity; tags: string[] }[]
  limitedSpent?: string[]
  marks?: { skill: string; count: number; source: number | null }[]
  equipment: Entity[]
  detachedEquipment?: string[]
  equipmentDisabledMarkers?: ('weapon' | 'armor' | 'defenseHorse' | 'attackHorse' | 'treasure')[]
  judges: Entity[]
  judgeAssignments: Entity[]
  logicalJudges?: { kind: string; material?: number }[]
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
  compare?: CompareParticipants
  compareGroup?: CompareGroup
  moveCards?: MoveChoiceView
  equipmentButtons?: { required: number; options: { id: number; kind: string; card?: number }[] }
}
interface EventRecord {
  bodySkipped?: boolean
  id: number
  type: string
  status: string
  parent: number | null
  seat?: number
  target?: number
  card?: Entity
  cards?: [Entity, Entity]
  result?: CompareResult
  amount?: number
  before?: number
  after?: number
  kind?: string
  skill?: string
  role?: keyof typeof roles
  nature?: DamageNature
  linked?: boolean
  turnedOver?: boolean
  delta?: number
}
export interface TableView {
  content?: ContentPreview
  mountCombined?: boolean
  roundSuspended?: boolean
  groupUpdates?: GroupAnnouncement[]
  version: number
  activeSeat: number
  humanSeat: number
  turnReason?: string
  phaseReason?: string
  waitingFor: number | null
  seats: SeatView[]
  turn: { round: number; number: number; phase: string; seat: number } | null
  counterWindow: boolean
  winner: keyof typeof sides | null
  drawn: boolean
  deckCount: number
  resolving: Entity[]
  knownSpecial?: Entity[]
  discard: Entity[]
  choice: Choice | null
  actions: Action[]
  events: EventRecord[]
  virtualPlays: { kind: string; branch: Branch }[]
  skillPlays: { skill: string; branch: Branch }[]
  activePlays?: { registration: number; skill: string; branch: Branch }[]
  privateChoiceCards: Entity[]
  cardFaces?: { id: number; suit: Card['suit']; rank: number }[]
  cardNatures?: Record<number, DamageNature>
}
const definition = (kind: string) => (kind === 'tiesuo' ? tiesuo : definitions[kind])
export function cardName(kind: string, nature?: DamageNature, content?: ContentPreview) {
  if (kind === 'sha' && nature) return nature === 'fire' ? '火杀' : '雷杀'
  return definition(kind)?.name ?? content?.cards.find((card) => card.id === kind)?.name ?? kind
}
export function faceOf(view: TableView, card: Entity): TableCard {
  const printed = view.cardFaces?.find((face) => face.id === card.id) ?? deck[card.id]
  const nature = card.kind === 'sha' ? view.cardNatures?.[card.id] : undefined
  const content = view.content?.cards.find((entry) => entry.id === card.kind)
  return {
    ...card,
    name: content?.name ?? cardName(card.kind, nature),
    suit: printed?.suit as Card['suit'] | undefined,
    rank: printed?.rank,
    description: nature
      ? `对攻击范围内的一名对手使用。若其未打出闪，造成1点${damageNames[nature]}伤害。`
      : content?.description ?? definition(card.kind)?.description ?? '',
  }
}
export function seatName(view: TableView, id: number | undefined) {
  const seat = view.seats.find((s) => s.id === id)
  return seat ? (generalDefinition(view, seat.hero)?.name ?? seat.hero) : '牌堆'
}
const suits = { spade: '♠', heart: '♥', club: '♣', diamond: '♦' }
export function printedFace(card: TableCard) {
  if (!card.suit || card.rank == null) return card.name
  const rank =
    ({ 1: 'A', 11: 'J', 12: 'Q', 13: 'K' } as Record<number, string>)[card.rank] ?? card.rank
  return `${suits[card.suit]}${rank} ${card.name}`
}

function comparedBids(
  view: TableView,
  seats: CompareParticipants,
  cards: [Entity, Entity],
  numbers?: [number, number],
) {
  return ([0, 1] as const).map((i) => {
    const seat = i === 0 ? seats.initiator : seats.target
    const card = faceOf(view, cards[i])
    return { seat, name: seatName(view, seat), card, face: printedFace(card), points: numbers?.[i] }
  })
}
export interface PublicComparison {
  event: number
  target?: number
  bids: ReturnType<typeof comparedBids>
  outcome?: string
  rounds?: { target: string; first: number; second: number }[]
}

/** Assemble only the card faces and scores that the engine has announced publicly. */
export function updateGroupComparison(view: TableView, previous: PublicComparison[], update: GroupAnnouncement): PublicComparison[] {
  const rows = previous.filter((item) => item.event === update.comparison)
  if (update.type === 'reveal') {
    const target = update.mode === 'pairwise' ? update.bids[1]?.seat : undefined
    const bids = update.bids.map(({ seat, card: entity }) => {
      const card = faceOf(view, entity)
      return { seat, name: seatName(view, seat), card, face: printedFace(card), points: undefined }
    })
    return [...rows.filter((row) => row.target !== target), { event: update.comparison, target, bids }]
  }
  if (update.type === 'pair') {
    return rows.map((row) => row.target !== update.target ? row : {
      ...row,
      outcome: update.winner == null ? '拼点平局' : `${seatName(view, update.winner)}拼点获胜`,
      bids: row.bids.map((bid, index) => ({ ...bid, points: update.numbers[index] })),
    })
  }
  if (update.mode === 'pairwise') return rows
  const first = update.numbers[0]?.[0]
  const varies = update.numbers.some(([points]) => points !== first)
  return rows.map((row) => ({
    ...row,
    outcome: update.winner == null ? '最高点并列' : `${seatName(view, update.winner)}拼点获胜`,
    rounds: varies ? update.numbers.map(([a, b], index) => ({ target: seatName(view, update.group.targets[index]), first: a, second: b })) : undefined,
    bids: row.bids.map((bid) => ({
      ...bid,
      points: bid.seat === update.group.initiator ? (varies ? undefined : first) : update.numbers[update.group.targets.indexOf(bid.seat)]?.[1],
    })),
  }))
}
export function publicComparison(view: TableView, e: EventRecord): PublicComparison | undefined {
  if (e.status !== 'started') return
  if (e.type === 'compareResult' && e.result) {
    const { event, participants, cards, numbers, winner } = e.result
    return {
      event,
      bids: comparedBids(view, participants, cards, numbers),
      outcome: winner == null ? '拼点平局' : `${seatName(view, winner)}拼点获胜`,
    }
  }
  if (e.type === 'compareReveal' && e.parent != null && e.seat != null && e.target != null && e.cards) {
    return {
      event: e.parent,
      bids: comparedBids(view, { initiator: e.seat, target: e.target }, e.cards),
    }
  }
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

function isPresentationEvent(e: EventRecord) {
  if (e.bodySkipped) return false
  if (e.type === 'draw') return e.status === 'finished' && (e.amount ?? 0) > 0
  if (e.type === 'gainMaxHp' || e.type === 'loseMaxHp' || e.type === 'changeHp')
    return e.status !== 'started' && e.before != null && e.after != null && e.before !== e.after
  if (['link', 'turnOver', 'changeArmor', 'damage', 'death'].includes(e.type)) return e.status === 'finished'
  return e.status === 'started'
}

export function visualEvents(view: TableView): FeedbackEvent[] {
  const cues: FeedbackEvent[] = []
  const announced = new Set<number>()
  for (const e of view.events) {
    if (!isPresentationEvent(e) || e.seat == null) continue
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
        cues.push({ type, seat: e.seat, cards: [faceOf(view, e.card)] })
      }
    }
  }
  return cues
}

export function counterPrompt(view: TableView): string | undefined {
  const info = view.choice?.counter
  if (!info) return
  const effect = `【${cardName(info.cardKind, undefined, view.content)}】对${seatName(view, info.target)}的效果`
  return info.negated
    ? `${effect}已被抵消，是否使用无懈可击恢复？`
    : `是否使用无懈可击抵消${effect}？`
}

export function describeEvent(view: TableView, e: EventRecord): string | undefined {
  if (!isPresentationEvent(e)) return
  const comparison = publicComparison(view, e)
  if (comparison) {
    const { bids, outcome } = comparison
    if (outcome) return `${outcome} · ${bids.map((b) => `${b.name} ${b.points}点`).join(' / ')}`
    return `拼点亮牌 · ${bids.map((b) => `${b.name}【${b.face}】`).join(' / ')}`
  }
  const who = seatName(view, e.seat)
  if (e.card && cardEffects[e.type] && e.type !== 'reveal') {
    const verb = e.type === 'equip' ? '装备' : cardEffects[e.type] === 'respond' ? '打出' : '使用'
    const target = e.target == null ? '' : ' → ' + seatName(view, e.target)
    return `${who}${verb}【${faceOf(view, e.card).name}】${target}`
  }
  if (e.type === 'useVirtual' && e.kind) return `${who}使用转化牌【${cardName(e.kind)}】`
  if (e.type === 'recastReveal' && e.card) {
    const reveals = e.parent == null ? [e] : view.events.filter(
      (event) => event.type === 'recastReveal' && event.parent === e.parent
        && event.card && isPresentationEvent(event),
    )
    if (reveals[0]?.id === e.id) {
      return `${who}重铸${reveals.map(({ card }) => card ? `【${faceOf(view, card).name}】` : '').join('、')}`
    }
    return
  }
  if (e.type === 'reveal' && e.card) return `${who}判定亮出【${faceOf(view, e.card).name}】`
  if (e.type === 'link') return `${who}${e.linked ? '横置' : '解除横置'}`
  if (e.type === 'turnOver') return `${who}的武将牌翻至${e.turnedOver ? '背面' : '正面'}`
  if (e.type === 'changeArmor' && e.delta)
    return `${who}的护甲${e.delta > 0 ? '增加' : '减少'}${Math.abs(e.delta)}点`
  if (e.type === 'damage' && e.nature && e.target != null)
    return `${seatName(view, e.target)}受到${e.amount}点${damageNames[e.nature]}伤害`
  if (e.type === 'changeHp' && e.before !== e.after) return `${who}体力 ${e.before} → ${e.after}`
  if (e.type === 'gainMaxHp' || e.type === 'loseMaxHp') return `${who}体力上限 ${e.before} → ${e.after}`
  if (e.type === 'death') return `${seatName(view, e.target)}阵亡${e.role ? ' · ' + roles[e.role] : ''}`
  if (e.type === 'draw') return `${who}摸了${e.amount}张牌`
  if (e.type === 'skill' && e.skill) {
    const skill = view.content?.generals.flatMap((general) => general.skills).find((entry) => entry.id === e.skill)
    return `${who}发动【${skill?.name ?? skillName(e.skill)}】`
  }
}
