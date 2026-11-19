<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { slot } from '@mak/rules'
import BattleEffects from './BattleEffects.vue'
import CardFace from './CardFace.vue'
import ClassicCompare from './ClassicCompare.vue'
import HealthMeter from './HealthMeter.vue'
import ClassicSelfEquipment from './ClassicSelfEquipment.vue'
import ClassicSeatZones from './ClassicSeatZones.vue'
import MoveChoice from './MoveChoice.vue'
import { heroPortrait } from './battleArt'
import drawBack from './assets/standard/table/draw-back.png'
import {
  cardName,
  counterPrompt,
  faceOf,
  factions,
  generalDefinition,
  phases,
  printedFace,
  roles,
  seatName,
  sides,
  skillName,
} from './classicProtocol'
import type { Action, Branch, CardPrint, DamageNature, Entity, MoveStep, SeatView, TableView } from './classicProtocol'
import type { FeedbackEvent } from './battleMotion'
import './classicBattle.css'

const props = defineProps<{
  view: TableView
  locked: boolean
  log: string[]
  feedback: { revision: number; events: FeedbackEvent[] }
  saveError: boolean
  error: string
  readonly?: boolean
}>()
const emit = defineEmits<{
  act: [action: Action, version: number]
  restart: []
  retrySave: []
}>()

const suitNames = { spade: '黑桃', heart: '红桃', club: '梅花', diamond: '方片' }
const responseLabels = new Set(['使用闪', '使用杀', '打出响应牌', '使用桃救援'])
const selectedCard = ref<number | null>(null)
const selectedCards = ref<number[]>([])
const selectedEquipmentButtons = ref<number[]>([])
const selectedTargets = ref<number[]>([])
const selectedBranch = ref<number | null>(null)
const selectedMode = ref<'card' | 'virtual' | 'skill' | 'active' | 'choice' | null>(null)
const selectedPlay = ref<number | null>(null)
const me = computed(() => props.view.seats.find((seat) => seat.id === props.view.humanSeat))
const others = computed(() => props.view.seats.filter((seat) => seat.id !== props.view.humanSeat))
// Seat positions follow the physical order around the table, starting at the human player.
function relativeSeat(id: number) {
  const seats = props.view.seats
  const human = seats.findIndex((seat) => seat.id === props.view.humanSeat)
  const index = seats.findIndex((seat) => seat.id === id)
  return human < 0 || index < 0 ? undefined : (index - human + seats.length) % seats.length
}
const inputReady = computed(
  () =>
    !props.locked &&
    props.view.winner === null &&
    !props.view.drawn &&
    props.view.waitingFor === props.view.humanSeat,
)
const hand = computed(() => (me.value?.hand ?? []).map((card) => faceOf(props.view, card)))
const selectedHand = computed(() => hand.value.find((card) => card.id === selectedCard.value))
const choice = computed(() =>
  props.view.choice?.seat === props.view.humanSeat ? props.view.choice : null,
)
const moveChoice = computed(() => choice.value?.moveCards)
const equipmentButtons = computed(() => choice.value?.equipmentButtons)
const active = computed(() => inputReady.value && !moveChoice.value)
const moveChoiceCards = computed(() =>
  (props.view.privateChoiceCards ?? []).map((card) => faceOf(props.view, card)),
)
const cardActions = computed(() =>
  props.view.actions.filter(
    (action): action is Extract<Action, { type: 'playCard' }> => action.type === 'playCard',
  ),
)
const recastActions = computed(() => props.view.actions.filter((action) => action.type === 'recast'))
const playable = computed(
  () => new Set([...cardActions.value, ...recastActions.value].map((action) => action.card)),
)
const branches = computed(() => (!equipmentButtons.value && choice.value?.kind === 'content' ? choice.value.branches : []))
const virtuals = computed(() => props.view.virtualPlays ?? [])
const skills = computed(() => props.view.skillPlays ?? [])
const activePlays = computed(() => props.view.activePlays ?? [])
const privateBid = computed(() => !!(choice.value?.compare || choice.value?.compareGroup))
type SkillMode = 'choice' | 'virtual' | 'skill' | 'active'
type SkillOption = { mode: SkillMode; index: number; label: string; skill: string }
const skillStack = computed(() => {
  const roster = generalDefinition(props.view, me.value?.hero ?? '')?.skills ?? []
  const byLabel = (label: string) => roster.find(({ name }) =>
    label === name || label.startsWith(name + '：') ||
    label.startsWith('发动' + name) || label.startsWith('发动【' + name + '】'),
  )
  const options: SkillOption[] = choice.value
    ? branches.value.flatMap((item) => {
        const skill = byLabel(item.label)
        return skill ? [{ mode: 'choice', index: item.id, label: item.label, skill: skill.id }] : []
      })
    : [
        ...virtuals.value.map((item, index) => ({
          mode: 'virtual' as const, index, label: item.branch.label,
          skill: byLabel(item.branch.label)?.id ?? item.branch.label,
        })),
        ...skills.value.map((item, index) => ({
          mode: 'skill' as const, index, label: item.branch.label, skill: item.skill,
        })),
        ...activePlays.value.map((item, index) => ({
          mode: 'active' as const, index, skill: item.skill,
          label: item.branch.label === item.skill ? skillName(item.skill) : item.branch.label,
        })),
      ]
  const stack = new Map(roster.map(({ id, name, description }) => [id, { name, description, options: [] as SkillOption[] }]))
  for (const option of options) {
    const entry = stack.get(option.skill) ?? { name: option.label, description: '', options: [] }
    entry.options.push(option)
    stack.set(option.skill, entry)
  }
  return [...stack.values()]
})
const branch = computed<Branch | null>(() => {
  if (privateBid.value) return choice.value?.branches[0] ?? null
  if (selectedMode.value === 'choice')
    return branches.value.find((item) => item.id === selectedBranch.value) ?? null
  if (selectedMode.value === 'virtual')
    return virtuals.value[selectedPlay.value ?? -1]?.branch ?? null
  if (selectedMode.value === 'skill') return skills.value[selectedPlay.value ?? -1]?.branch ?? null
  if (selectedMode.value === 'active') return activePlays.value[selectedPlay.value ?? -1]?.branch ?? null
  return null
})
const handOptions = computed(() => new Set(choice.value?.cards ?? []))
const branchCards = computed(() => new Set(branch.value?.cards ?? []))
type PlayedFace = { id?: number; event?: number; face: CardPrint; nature?: DamageNature }
const resolving = computed<PlayedFace[]>(() => {
  const virtuals = props.view.resolvingVirtuals ?? []
  const materials = new Set(virtuals.flatMap((card) => card.materials))
  const exposed = props.view.resolving.filter((card) =>
    !branchCards.value.has(card.id) && !materials.has(card.id),
  )
  return [
    ...exposed.map((card) => ({
      id: card.id,
      face: faceOf(props.view, card),
      nature: props.view.cardNatures?.[card.id],
    })),
    ...virtuals.map(({ event, kind, nature }) => ({
      event,
      face: { kind, name: cardName(kind, nature, props.view.content) },
      nature,
    })),
  ]
})
const handControls = ref<HTMLElement | null>(null)
const handResponse = computed(() => {
  const request = choice.value
  return request?.kind === 'shan' || request?.kind === 'save' ? request : null
})
const singleResponseCard = computed(() =>
  handResponse.value?.cards.length === 1 ? handResponse.value.cards[0] : undefined,
)
const currentActions = computed(() =>
  cardActions.value.filter((action) => action.card === selectedCard.value),
)
// Ordinary equipment comes with its wearer already determined by the engine.
const selfEquipAction = computed(() => {
  if (selectedMode.value !== 'card' || !selectedHand.value || !slot(selectedHand.value.kind)) return
  const [action] = currentActions.value
  if (currentActions.value.length === 1 && action?.targets.length === 1 && action.targets[0] === me.value?.id)
    return action
})
const selectedRecast = computed(() =>
  selectedMode.value === 'card' && !choice.value
    ? recastActions.value.find((action) => action.card === selectedCard.value)
    : undefined,
)
const unorderedTargets = computed(
  () => selectedMode.value === 'card' && selectedHand.value?.kind === 'tiesuo',
)
const matchingActions = computed(() =>
  currentActions.value.filter((action) =>
    selectedTargets.value.every((id, index) =>
      unorderedTargets.value ? action.targets.includes(id) : action.targets[index] === id,
    ),
  ),
)
const possibleTargets = computed(() => {
  if (branch.value) return new Set(branch.value.targets)
  if (selfEquipAction.value) return new Set<number>()
  return new Set(
    matchingActions.value.flatMap((action) =>
      unorderedTargets.value
        ? action.targets.filter((id) => !selectedTargets.value.includes(id))
        : action.targets.slice(selectedTargets.value.length, selectedTargets.value.length + 1),
    ),
  )
})
const exactAction = computed(() =>
  selfEquipAction.value ?? matchingActions.value.find((action) => action.targets.length === selectedTargets.value.length),
)
const targetOrder = computed(() => exactAction.value?.targets ?? selectedTargets.value)
const canConfirm = computed(() => {
  if (!active.value) return false
  if (equipmentButtons.value) return selectedEquipmentButtons.value.length === equipmentButtons.value.required
  if (choice.value?.kind === 'discard')
    return (
      selectedCards.value.length >= choice.value.min &&
      selectedCards.value.length <= choice.value.max
    )
  if (choice.value?.kind === 'shan' || choice.value?.kind === 'save')
    return selectedCard.value !== null && handOptions.value.has(selectedCard.value)
  if (branch.value)
    return (
      selectedCards.value.length >= branch.value.minCards &&
      selectedCards.value.length <= branch.value.maxCards &&
      selectedTargets.value.length >= branch.value.minTargets &&
      selectedTargets.value.length <= branch.value.maxTargets
    )
  return !!exactAction.value
})
const ownZoneCards = computed(() => [
  ...(me.value?.equipment ?? []),
  ...(me.value?.judges ?? []),
  ...(me.value?.expansions ?? []).map(({ card }) => card),
])
const ownCards = computed(() => new Set([...hand.value, ...ownZoneCards.value].map((card) => card.id)))
const galleryCards = computed(() => [
  ...hand.value,
  ...ownZoneCards.value
    .filter((card) => branchCards.value.has(card.id))
    .map((card) => faceOf(props.view, card)),
])
const pool = computed(() => {
  const ids = (branch.value?.cards ?? []).filter((id) => !ownCards.value.has(id))
  const visible: Entity[] = [
    ...props.view.resolving,
    ...props.view.discard,
    ...others.value.flatMap((seat) => [
      ...seat.equipment, ...seat.judges, ...(seat.knownHand ?? []),
      ...(seat.expansions ?? []).map(({ card }) => card),
    ]),
    ...(props.view.privateChoiceCards ?? []),
  ]
  return ids.map((id) => {
    const entity = visible.find((card) => card.id === id)
    return { id, card: entity ? faceOf(props.view, entity) : undefined }
  })
})
const randomHandBranch = computed(() =>
  branches.value.find((item) => item.label === '手牌区（随机）' && item.maxCards === 0),
)
const randomHandCount = computed(() => randomHandBranch.value
  ? props.view.seats.find((seat) => seat.id === choice.value?.target)?.handCount ?? 0
  : 0)
const centerChoice = computed(() => !!moveChoice.value || pool.value.length > 0 || randomHandCount.value > 0)
const selectedRandomBack = ref<number | null>(null)
const selectedPreview = computed(() => !choice.value
  ? selectedHand.value ?? galleryCards.value.find((card) => selectedCards.value.includes(card.id))
  : undefined)
const showConfirm = computed(() => {
  if (equipmentButtons.value || branch.value || choice.value?.kind === 'discard') return true
  if (handResponse.value) return handResponse.value.cards.length > 1
  return !choice.value && (!selectedRecast.value || currentActions.value.length > 0)
})
const showCancel = computed(() => !!selectedMode.value || selectedCard.value !== null ||
  selectedCards.value.length > 0 || selectedEquipmentButtons.value.length > 0)
const selectionProgress = computed(() => {
  if (!branch.value || privateBid.value) return ''
  const { minCards, maxCards, minTargets, maxTargets } = branch.value
  const progress = []
  if (maxCards) progress.push(`选牌 ${selectedCards.value.length}/${minCards === maxCards ? maxCards : `${minCards}–${maxCards}`}`)
  if (maxTargets) progress.push(`目标 ${selectedTargets.value.length}/${minTargets === maxTargets ? maxTargets : `${minTargets}–${maxTargets}`}`)
  return progress.join(' · ')
})
const prompt = computed(() => {
  if (props.view.drawn) return '牌堆耗尽，本局平局'
  if (props.view.winner) return `${sides[props.view.winner]}获胜`
  if (props.view.roundSuspended) return '所有角色均已离场，等待下一轮'
  if (props.readonly) return '录像回放'
  if (props.view.waitingFor === props.view.humanSeat && props.locked) return '正在结算…'
  if (!inputReady.value)
    return `${seatName(props.view, props.view.waitingFor ?? props.view.activeSeat)}正在思考`
  const counter = counterPrompt(props.view)
  if (counter) return counter
  if (choice.value?.compareGroup) {
    const group = choice.value.compareGroup
    const others = [group.initiator, ...group.targets].filter((id) => id !== choice.value?.seat)
    return `与${others.map((id) => seatName(props.view, id)).join('、')}拼点，暗选一张手牌`
  }
  if (choice.value?.compare) {
    const { initiator, target } = choice.value.compare
    const other = choice.value.seat === initiator ? target : initiator
    return `与${seatName(props.view, other)}拼点，暗选一张手牌`
  }
  if (choice.value?.prompt) return choice.value.prompt
  if (choice.value?.kind === 'discard') return `请选择 ${choice.value.min} 张牌弃置`
  if (choice.value?.kind === 'shan') return '请打出闪，或选择放弃'
  if (choice.value?.kind === 'save')
    return `是否救援 ${seatName(props.view, choice.value.target ?? undefined)}？`
  if (randomHandCount.value) return `选择${seatName(props.view, choice.value?.target ?? undefined)}的牌（暗牌随机结算）`
  if (choice.value?.kind === 'content') return branch.value?.label ?? '请选择一项并完成所需选择'
  if (selfEquipAction.value) return `确认装备${selectedHand.value!.name}`
  if (branch.value) return branch.value.label
  if (selectedRecast.value) {
    if (!currentActions.value.length) return '可重铸此牌'
    if (exactAction.value && possibleTargets.value.size) return '可继续选择目标，也可确认使用或重铸'
    return exactAction.value ? '确认使用此牌，或选择重铸' : '选择目标使用此牌，或直接重铸'
  }
  if (selectedMode.value === 'card' && selectedCard.value !== null) return '请选择目标，再确认使用'
  return '选择手牌、技能或结束出牌'
})
function reset() {
  selectedCard.value = null
  selectedCards.value = []
  selectedEquipmentButtons.value = []
  selectedTargets.value = []
  selectedBranch.value = null
  selectedMode.value = null
  selectedPlay.value = null
  selectedRandomBack.value = null
}
watch(() => props.view.version, reset)
watch(() => JSON.stringify([choice.value?.event, choice.value?.equipmentButtons]), reset)
function setMode(mode: SkillMode, index: number) {
  reset()
  selectedMode.value = mode
  if (mode === 'choice') selectedBranch.value = index
  else selectedPlay.value = index
}
function isModeSelected(option: SkillOption) {
  return selectedMode.value === option.mode &&
    (option.mode === 'choice' ? selectedBranch.value : selectedPlay.value) === option.index
}
function chooseSkill(option: SkillOption) {
  if (option.mode !== 'choice') {
    setMode(option.mode, option.index)
    return
  }
  const item = branches.value.find(({ id }) => id === option.index)
  if (item) chooseBranch(item)
}
function submitBranch(item: Branch, cards: number[], targets: number[]) {
  if (active.value && choice.value)
    emit('act', { type: 'select', event: choice.value.event, branch: item.id, cards, targets }, props.view.version)
}
function chooseBranch(item: Branch) {
  if (!active.value) return
  if (!item.maxTargets && !item.maxCards) {
    submitBranch(item, [], [])
  } else if (item.id === 1 && !item.maxTargets && item.minCards === 1 && item.maxCards === 1 && item.cards.length === 1 &&
    (responseLabels.has(item.label) ||
      (choice.value?.counter && item.label.startsWith('无懈可击：')))) {
    submitBranch(item, item.cards, [])
  } else {
    setMode('choice', item.id)
  }
}
function chooseRandomBack(index: number) {
  if (!active.value || !randomHandBranch.value) return
  if (selectedRandomBack.value === index) {
    reset()
    return
  }
  setMode('choice', randomHandBranch.value.id)
  selectedRandomBack.value = index
}
function toggle(list: number[], id: number, max: number): number[] {
  if (list.includes(id)) return list.filter((item) => item !== id)
  return list.length < max ? [...list, id] : list
}
function chooseHand(id: number) {
  if (!active.value || equipmentButtons.value) return
  if (branch.value) {
    if (!branchCards.value.has(id)) return
    if (privateBid.value) selectedCards.value = selectedCards.value.includes(id) ? [] : [id]
    else selectedCards.value = toggle(selectedCards.value, id, branch.value.maxCards)
  } else if (choice.value?.kind === 'discard') {
    if (handOptions.value.has(id))
      selectedCards.value = toggle(selectedCards.value, id, choice.value.max)
  } else if (choice.value?.kind === 'shan' || choice.value?.kind === 'save') {
    if (handOptions.value.has(id)) selectedCard.value = selectedCard.value === id ? null : id
  } else if (!choice.value && playable.value.has(id)) {
    const deselect = selectedMode.value === 'card' && selectedCard.value === id
    reset()
    if (deselect) return
    selectedMode.value = 'card'
    selectedCard.value = id
    const options = cardActions.value.filter((action) => action.card === id)
    if (options.length === 1 && options[0]!.targets.length > 1)
      selectedTargets.value = [...options[0]!.targets]
  }
}
function choosePool(id: number) {
  if (active.value && branch.value && branchCards.value.has(id))
    selectedCards.value = toggle(selectedCards.value, id, branch.value.maxCards)
}
function chooseSeat(id: number) {
  if (!active.value || equipmentButtons.value) return
  const index = selectedTargets.value.indexOf(id)
  if (index >= 0) {
    selectedTargets.value = branch.value || unorderedTargets.value
      ? selectedTargets.value.filter((target) => target !== id)
      : selectedTargets.value.slice(0, index)
    return
  }
  if (!possibleTargets.value.has(id)) return
  const max =
    branch.value?.maxTargets ??
    Math.max(0, ...currentActions.value.map((action) => action.targets.length))
  if (selectedTargets.value.length < max) selectedTargets.value.push(id)
}
function confirm() {
  if (!canConfirm.value) return
  let action: Action | undefined
  if (choice.value && equipmentButtons.value)
    action = { type: 'selectEquipmentButtons', event: choice.value.event, buttons: [...selectedEquipmentButtons.value] }
  else if (choice.value?.kind === 'discard')
    action = { type: 'discard', event: choice.value.event, cards: [...selectedCards.value] }
  else if (handResponse.value && selectedCard.value !== null)
    return submitResponse(selectedCard.value)
  else if (choice.value?.kind === 'content' && branch.value)
    return submitBranch(branch.value, [...selectedCards.value], [...selectedTargets.value])
  else if (selectedMode.value === 'virtual' && branch.value && selectedPlay.value !== null)
    action = {
      type: 'playVirtual',
      kind: virtuals.value[selectedPlay.value]!.kind,
      cards: [...selectedCards.value],
      targets: [...selectedTargets.value],
    }
  else if (selectedMode.value === 'skill' && branch.value && selectedPlay.value !== null)
    action = {
      type: 'useSkill',
      skill: skills.value[selectedPlay.value]!.skill,
      cards: [...selectedCards.value],
      targets: [...selectedTargets.value],
    }
  else if (selectedMode.value === 'active' && branch.value && selectedPlay.value !== null)
    action = {
      type: 'activateSkill',
      registration: activePlays.value[selectedPlay.value]!.registration,
      cards: [...selectedCards.value],
      targets: [...selectedTargets.value],
    }
  else action = exactAction.value
  if (action) emit('act', action, props.view.version)
}
function recast() {
  if (active.value && selectedRecast.value) emit('act', selectedRecast.value, props.view.version)
}
function confirmMove(moves: MoveStep[] | null) {
  if (inputReady.value && choice.value?.moveCards) {
    emit('act', { type: 'moveCards', event: choice.value.event, moves }, props.view.version)
  }
}
function submitResponse(card: number | null) {
  if (active.value && handResponse.value)
    emit('act', { type: 'choose', event: handResponse.value.event, card }, props.view.version)
}
function seatCards(seat: SeatView, zone: 'equipment' | 'judges') {
  return seat[zone].map((card) =>
    faceOf(
      props.view,
      zone === 'judges'
        ? {
            ...card,
            kind: seat.judgeAssignments?.find((item) => item.id === card.id)?.kind ?? card.kind,
          }
        : card,
    ),
  )
}
function isCardSelected(id: number) {
  return selectedCard.value === id || selectedCards.value.includes(id)
}
function canPickCard(id: number) {
  if (!active.value || equipmentButtons.value || singleResponseCard.value !== undefined) return false
  if (branch.value) return branchCards.value.has(id)
  if (choice.value) return handOptions.value.has(id)
  return playable.value.has(id)
}
</script>

<template>
  <section
    class="classic-board"
    :class="{ 'classic-board-eight': view.seats.length === 8 }"
    :data-revision="view.version"
    :data-waiting-for="view.waitingFor"
    :data-phase="view.turn?.phase"
    :data-choice-kind="moveChoice ? 'move' : choice?.kind"
    :data-result="view.winner ?? (view.drawn ? 'draw' : null)"
    data-battle-table
  >
    <BattleEffects :view="feedback" />
    <div class="classic-topline">
      <span>第 {{ view.turn?.round ?? 1 }} 轮 · 第 {{ view.turn?.number ?? 1 }} 回合<span v-if="view.turnReason" data-turn-reason> · {{ skillName(view.turnReason) }}额外回合</span></span>
      <span
        >{{ phases[view.turn?.phase ?? ''] ?? view.turn?.phase }}<span v-if="view.phaseReason" data-phase-reason> · {{ skillName(view.phaseReason) }}额外阶段</span> ·
        {{ seatName(view, view.activeSeat) }}</span
      >
      <span>牌堆 {{ view.deckCount }} · 弃牌 {{ view.discard.length }}</span>
      <details class="classic-log">
        <summary>对局记录</summary>
        <ol>
          <li v-for="(line, i) in [...log].reverse()" :key="i">{{ line }}</li>
        </ol>
      </details>
      <ClassicCompare :view="view" />
      <details class="classic-roster-skills">
        <summary>武将技能</summary>
        <ol>
          <li v-for="seat in view.seats" :key="seat.id">
            {{ seatName(view, seat.id) }}：{{
              generalDefinition(view, seat.hero)?.skills.map((skill) => `${skill.name}：${skill.description}`).join(' ')
            }}
          </li>
        </ol>
      </details>
    </div>
    <div class="classic-opponents" aria-label="其他角色">
      <div
        v-for="seat in others"
        :key="seat.id"
        class="classic-seat"
        :class="{
          dead: !seat.alive,
          current: seat.id === view.activeSeat,
          targetable: possibleTargets.has(seat.id),
          chosen: selectedTargets.includes(seat.id),
        }"
        :data-seat="seat.id"
        :data-position="view.seats.length === 8 ? relativeSeat(seat.id) : undefined"
      >
        <button
          class="seat-select"
          :disabled="
            !active || (!possibleTargets.has(seat.id) && !selectedTargets.includes(seat.id))
          "
          :aria-pressed="selectedTargets.includes(seat.id)"
          @click="chooseSeat(seat.id)"
        >
          <img v-if="heroPortrait(seat.hero)" :src="heroPortrait(seat.hero)" :alt="`${seatName(view, seat.id)}立绘`" />
          <span class="seat-identity">{{
            seat.alive
              ? seat.role
                ? roles[seat.role]
                : '身份未知'
              : `阵亡 · ${seat.role ? roles[seat.role] : '身份未知'}`
          }}</span>
          <span class="seat-name">{{ seatName(view, seat.id) }}</span>
          <span class="seat-faction"
            >{{ factions[generalDefinition(view, seat.hero)?.faction ?? ''] ?? '' }} · 手牌
            {{ seat.handCount }}</span
          >
          <HealthMeter :hp="seat.hp" :max-hp="seat.maxHp" />
          <span v-if="selectedTargets.includes(seat.id)" class="target-number">{{
            targetOrder.indexOf(seat.id) + 1
          }}</span>
        </button>
        <ClassicSeatZones
          :label="seatName(view, seat.id)"
          :seat="seat"
          :mount-combined="view.mountCombined"
          :seat-names="view.seats.map((item) => seatName(view, item.id))"
          :equipment="seatCards(seat, 'equipment')"
          :judges="seatCards(seat, 'judges')"
          :known-hand="seat.knownHand?.map((card) => faceOf(view, card))"
          :expansions="seat.expansions?.map((item) => ({ ...item, card: faceOf(view, item.card) }))"
        />
      </div>
    </div>
    <div class="classic-action-area" :class="{ 'is-centered': centerChoice || resolving.length > 0 }">
      <div
        class="classic-center"
        :class="{
          'has-choice': centerChoice,
          'has-preview': selectedPreview && !centerChoice,
          'has-move': moveChoice,
          'has-resolving': resolving.length > 0,
        }"
      >
        <p v-if="!centerChoice && !choice && !resolving.length" class="classic-turn">
          {{
            view.turn?.seat === view.humanSeat
              ? '你的回合'
              : `${seatName(view, view.turn?.seat)}的回合`
          }}
          · {{ phases[view.turn?.phase ?? ''] ?? view.turn?.phase }}
        </p>
        <section
          class="classic-center-surface"
          :data-card-choice="centerChoice ? '' : undefined"
          aria-label="当前操作"
        >
          <MoveChoice
            v-if="moveChoice && !readonly && handControls"
            :key="choice?.event"
            :request="moveChoice"
            :cards="moveChoiceCards"
            :controls-target="handControls"
            :disabled="!inputReady"
            :error="error"
            @confirm="confirmMove"
          />
          <template v-else>
            <h1 aria-live="polite">{{ prompt }}</h1>
            <div v-if="resolving.length || randomHandCount || pool.length" class="classic-pool">
              <div v-if="resolving.length" class="classic-resolving" role="group" aria-label="已打出的牌">
                <div
                  v-for="{ id, event, face, nature } in resolving"
                  :key="id ?? `virtual-${event}`"
                  class="playing-card classic-choice-card"
                  :class="{
                    red: face.suit === 'heart' || face.suit === 'diamond',
                    'long-name': face.name.length > 2,
                  }"
                  :data-resolving-card="id"
                  :data-resolving-virtual="event"
                  :data-nature="nature"
                  :aria-label="printedFace(face)"
                  role="img"
                >
                  <CardFace :card="face" />
                </div>
              </div>
              <div v-if="randomHandCount" class="classic-random-hand" role="group" aria-label="手牌区（随机）">
                <button
                  v-for="index in randomHandCount"
                  :key="index"
                  type="button"
                  class="playing-card classic-choice-card card-back"
                  :class="{ selected: selectedRandomBack === index }"
                  :aria-label="`暗牌 ${index}（随机结算）`"
                  :aria-pressed="selectedRandomBack === index"
                  :disabled="!active"
                  @click="chooseRandomBack(index)"
                >
                  <img :src="drawBack" alt="" />
                </button>
              </div>
              <div v-if="pool.length" role="group" aria-label="候选牌">
                <button
                  v-for="({ id, card }, index) in pool"
                  :key="id"
                  type="button"
                  class="playing-card classic-choice-card"
                  :data-card-id="id"
                  :data-nature="card ? view.cardNatures?.[id] : undefined"
                  :class="{
                    selected: isCardSelected(id),
                    'card-back': !card,
                    red: card?.suit === 'heart' || card?.suit === 'diamond',
                    'long-name': (card?.name.length ?? 0) > 2,
                  }"
                  :aria-label="card ? printedFace(card) : `暗牌 ${index + 1}`"
                  :aria-pressed="isCardSelected(id)"
                  :title="card?.description"
                  :disabled="!active"
                  @click="choosePool(id)"
                >
                  <CardFace v-if="card" :card="card" />
                  <img v-else :src="drawBack" alt="" />
                </button>
              </div>
            </div>
          </template>
        </section>
        <div v-if="saveError" class="classic-save-error">
          战绩保存失败 <button @click="emit('retrySave')">重试</button>
        </div>
        <button v-if="!readonly && (view.winner || view.drawn)" class="primary-button" @click="emit('restart')">
          再来一局
        </button>
        <slot />
      </div>
      <div ref="handControls" class="classic-hand-toolbar">
        <div v-if="selectedPreview && !centerChoice" class="classic-card-rule" data-hand-rule>
          <strong>{{ selectedPreview.name }}</strong>
          <span>{{ selectedPreview.description }}</span>
        </div>
        <div v-if="!moveChoice && !readonly && !view.winner && !view.drawn" class="classic-controls" aria-label="对局操作">
          <div v-if="branches.length > Number(!!randomHandCount) && !privateBid" class="classic-options">
            <button
              v-for="item in branches.filter(item => !randomHandCount || item.id !== randomHandBranch?.id)"
              :key="item.id"
              type="button"
              :class="{ selected: selectedMode === 'choice' && selectedBranch === item.id }"
              :disabled="!active"
              @click="chooseBranch(item)"
            >
              {{ item.label }}
            </button>
          </div>
          <div v-if="choice && branch?.maxTargets" class="classic-options classic-choice-targets" aria-label="选择目标">
            <button
              v-for="target in branch.targets"
              :key="target"
              type="button"
              :class="{ selected: selectedTargets.includes(target) }"
              :aria-pressed="selectedTargets.includes(target)"
              :disabled="!active"
              @click="chooseSeat(target)"
            >
              {{ seatName(view, target) }}
            </button>
          </div>
          <div v-if="showCancel || handResponse || view.actions.some(a => a.type === 'endPlay') || showConfirm" class="classic-control-row">
            <span v-if="selectionProgress" class="classic-choice-progress" aria-live="polite">
              {{ selectionProgress }}
            </span>
            <button
              v-if="showCancel"
              type="button"
              class="secondary"
              :disabled="!active"
              @click="reset"
            >
              取消
            </button>
            <template v-if="handResponse">
              <button type="button" class="secondary" :disabled="!active" @click="submitResponse(null)">放弃</button>
              <button
                v-if="singleResponseCard !== undefined"
                type="button"
                :disabled="!active"
                @click="submitResponse(singleResponseCard)"
              >
                {{ handResponse.kind === 'shan' ? '使用闪' : '使用桃' }}
              </button>
            </template>
            <button
              v-if="!choice && !selectedMode && view.actions.some((a) => a.type === 'endPlay')"
              type="button"
              class="secondary whitespace-nowrap"
              :disabled="!active"
              @click="emit('act', { type: 'endPlay' }, view.version)"
            >
              结束出牌
            </button>
            <button
              v-if="selectedRecast"
              type="button"
              :class="{ secondary: !!exactAction }"
              :disabled="!active"
              @click="recast"
            >
              重铸
            </button>
            <button v-if="showConfirm" type="button" :disabled="!canConfirm" @click="confirm">确认</button>
          </div>
        </div>
      </div>
    </div>
    <div class="classic-bottom">
      <ClassicSelfEquipment
        v-if="me"
        :seat="me"
        :equipment="seatCards(me, 'equipment')"
        :mount-combined="view.mountCombined"
      />
      <div class="classic-hand-zone">
        <div class="classic-hand-heading">
          <span>手牌 · {{ hand.length }}</span>
          <span v-if="equipmentButtons">选择装备 · {{ selectedEquipmentButtons.length }}/{{ equipmentButtons.required }}</span>
          <span v-else-if="selectedCards.length && !privateBid">已选 {{ selectedCards.length }} 张</span>
        </div>
        <div class="classic-hand" data-hand data-hand-gallery>
          <button
            v-for="card in galleryCards"
            :key="card.id"
            :data-card-id="card.id"
            :data-nature="view.cardNatures?.[card.id]"
            class="playing-card"
            :class="{
              red: card.suit === 'heart' || card.suit === 'diamond',
              'long-name': card.name.length > 2,
              selected: isCardSelected(card.id),
              playable: canPickCard(card.id),
            }"
            :disabled="!canPickCard(card.id)"
            :aria-pressed="isCardSelected(card.id)"
            :aria-label="[card.name, card.suit && suitNames[card.suit], card.rank, card.description].filter(Boolean).join(' ')"
            @click="chooseHand(card.id)"
          >
            <CardFace :card="card" />
          </button>
          <button
            v-for="option in equipmentButtons?.options ?? []"
            :key="`equipment-${option.id}`"
            type="button"
            :data-equipment-button="option.id"
            class="playing-card playable"
            :class="{
              selected: selectedEquipmentButtons.includes(option.id),
              'long-name': cardName(option.kind).length > 2,
            }"
            :aria-label="cardName(option.kind) + (option.card === undefined ? '（效果仍生效）' : '')"
            :aria-pressed="selectedEquipmentButtons.includes(option.id)"
            :disabled="!active"
            @click="selectedEquipmentButtons = toggle(selectedEquipmentButtons, option.id, equipmentButtons!.required)"
          >
            <CardFace v-if="option.card !== undefined" :card="faceOf(view, { id: option.card, kind: option.kind })" />
            <template v-else>
              <strong>{{ cardName(option.kind) }}</strong>
              <small>效果仍生效</small>
            </template>
          </button>
        </div>
      </div>
      <div v-if="me" class="classic-own-skills" aria-label="自己的技能">
        <template v-for="entry in skillStack" :key="entry.name">
          <button
            v-for="option in entry.options"
            :key="`${option.mode}-${option.index}`"
            type="button"
            :title="entry.description || option.label"
            :aria-label="option.mode === 'choice' ? `技能：${option.label}` : option.label"
            :aria-pressed="isModeSelected(option)"
            :disabled="!active || (privateBid && option.mode === 'choice')"
            @click="chooseSkill(option)"
          >
            {{ option.label }}
          </button>
          <button v-if="!entry.options.length" type="button" :title="entry.description" disabled>
            {{ entry.name }}
          </button>
        </template>
      </div>
      <div
        v-if="me"
        class="classic-self"
        :data-seat="me.id"
        :class="{ targetable: possibleTargets.has(me.id), chosen: selectedTargets.includes(me.id) }"
      >
        <button
          class="self-select"
          :disabled="!active || (!possibleTargets.has(me.id) && !selectedTargets.includes(me.id))"
          :aria-pressed="selectedTargets.includes(me.id)"
          @click="chooseSeat(me.id)"
        >
          <img v-if="heroPortrait(me.hero)" :src="heroPortrait(me.hero)" :alt="`${seatName(view, me.id)}立绘`" />
          <span class="self-identity">{{ me.role ? roles[me.role] : '身份未知' }}</span>
          <span class="self-name">{{ seatName(view, me.id) }}</span>
          <HealthMeter :hp="me.hp" :max-hp="me.maxHp" />
          <span v-if="selectedTargets.includes(me.id)" class="target-number">{{
            targetOrder.indexOf(me.id) + 1
          }}</span>
        </button>
        <ClassicSeatZones
          :label="seatName(view, me.id)"
          :seat="me"
          :mount-combined="view.mountCombined"
          :seat-names="view.seats.map((item) => seatName(view, item.id))"
          :equipment="seatCards(me, 'equipment')"
          hide-equipment
          :judges="seatCards(me, 'judges')"
          :expansions="me.expansions?.map((item) => ({ ...item, card: faceOf(view, item.card) }))"
        />
      </div>
    </div>
  </section>
</template>
