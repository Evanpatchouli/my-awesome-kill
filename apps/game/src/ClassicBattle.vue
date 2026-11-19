<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import BattleEffects from './BattleEffects.vue'
import CardFace from './CardFace.vue'
import ClassicCompare from './ClassicCompare.vue'
import HealthMeter from './HealthMeter.vue'
import ClassicSeatZones from './ClassicSeatZones.vue'
import MoveChoice from './MoveChoice.vue'
import { heroPortrait } from './battleArt'
import {
  cardName,
  counterPrompt,
  faceOf,
  factions,
  generalById,
  phases,
  printedFace,
  roles,
  seatName,
  sides,
  skillName,
} from './classicProtocol'
import type { Action, Branch, Entity, MoveStep, SeatView, TableView } from './classicProtocol'
import type { FeedbackEvent } from './battleMotion'
import './classicBattle.css'

const props = defineProps<{
  view: TableView
  locked: boolean
  log: string[]
  feedback: { revision: number; events: FeedbackEvent[] }
  saveError: boolean
  error: string
}>()
const emit = defineEmits<{
  act: [action: Action, version: number]
  restart: []
  retrySave: []
}>()

const selectedCard = ref<number | null>(null)
const selectedCards = ref<number[]>([])
const selectedEquipmentButtons = ref<number[]>([])
const selectedTargets = ref<number[]>([])
const selectedBranch = ref<number | null>(null)
const selectedMode = ref<'card' | 'virtual' | 'skill' | 'active' | 'choice' | null>(null)
const selectedPlay = ref<number | null>(null)
const me = computed(() => props.view.seats.find((seat) => seat.id === props.view.humanSeat))
const others = computed(() => props.view.seats.filter((seat) => seat.id !== props.view.humanSeat))
const inputReady = computed(
  () =>
    !props.locked &&
    props.view.winner === null &&
    !props.view.drawn &&
    props.view.waitingFor === props.view.humanSeat,
)
const hand = computed(() => (me.value?.hand ?? []).map((card) => faceOf(props.view, card)))
const resolving = computed(() => props.view.resolving.map((card) => faceOf(props.view, card)))
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
const currentActions = computed(() =>
  cardActions.value.filter((action) => action.card === selectedCard.value),
)
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
  return new Set(
    matchingActions.value.flatMap((action) =>
      unorderedTargets.value
        ? action.targets.filter((id) => !selectedTargets.value.includes(id))
        : action.targets.slice(selectedTargets.value.length, selectedTargets.value.length + 1),
    ),
  )
})
const exactAction = computed(() =>
  matchingActions.value.find((action) => action.targets.length === selectedTargets.value.length),
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
const pool = computed(() => {
  if (!branch.value) return []
  const own = new Set(hand.value.map((card) => card.id))
  const visible: Entity[] = [
    ...props.view.resolving,
    ...props.view.discard,
    ...props.view.seats.flatMap((seat) => [
      ...seat.equipment, ...seat.judges, ...(seat.knownHand ?? []),
    ]),
    ...(props.view.privateChoiceCards ?? []),
  ]
  return branch.value.cards
    .filter((id) => !own.has(id))
    .map((id) => {
      const entity = visible.find((card) => card.id === id)
      return entity ? faceOf(props.view, entity) : { id, name: '未知牌', rank: undefined }
    })
})
const prompt = computed(() => {
  if (props.view.drawn) return '牌堆耗尽，本局平局'
  if (props.view.winner) return `${sides[props.view.winner]}获胜`
  if (props.view.roundSuspended) return '所有角色均已离场，等待下一轮'
  if (props.view.waitingFor === props.view.humanSeat && props.locked) return '正在结算…'
  if (!active.value)
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
  if (choice.value?.kind === 'content') return '请选择一项并完成所需选择'
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
}
watch(() => props.view.version, reset)
watch(() => JSON.stringify([choice.value?.event, choice.value?.equipmentButtons]), reset)
function setMode(mode: 'choice' | 'virtual' | 'skill' | 'active', index: number) {
  reset()
  selectedMode.value = mode
  if (mode === 'choice') selectedBranch.value = index
  else selectedPlay.value = index
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
    reset()
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
  else if (
    (choice.value?.kind === 'shan' || choice.value?.kind === 'save') &&
    selectedCard.value !== null
  )
    action = { type: 'choose', event: choice.value.event, card: selectedCard.value }
  else if (choice.value?.kind === 'content' && branch.value)
    action = {
      type: 'select',
      event: choice.value.event,
      branch: branch.value.id,
      cards: [...selectedCards.value],
      targets: [...selectedTargets.value],
    }
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
function pass() {
  if (
    active.value &&
    choice.value &&
    (choice.value.kind === 'shan' || choice.value.kind === 'save')
  )
    emit('act', { type: 'choose', event: choice.value.event, card: null }, props.view.version)
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
  if (!active.value || equipmentButtons.value) return false
  if (branch.value) return branchCards.value.has(id)
  if (choice.value) return handOptions.value.has(id)
  return playable.value.has(id)
}
</script>

<template>
  <section
    class="classic-board"
    :data-revision="view.version"
    :data-waiting-for="view.waitingFor"
    :data-phase="view.turn?.phase"
    :data-choice-kind="moveChoice ? 'move' : choice?.kind"
    :inert="!!moveChoice"
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
      >
        <button
          class="seat-select"
          :disabled="
            !active || (!possibleTargets.has(seat.id) && !selectedTargets.includes(seat.id))
          "
          :aria-pressed="selectedTargets.includes(seat.id)"
          @click="chooseSeat(seat.id)"
        >
          <img :src="heroPortrait(seat.hero)" :alt="`${seatName(view, seat.id)}立绘`" />
          <span class="seat-identity">{{
            seat.alive
              ? seat.role
                ? roles[seat.role]
                : '身份未知'
              : `阵亡 · ${seat.role ? roles[seat.role] : '身份未知'}`
          }}</span>
          <span class="seat-name">{{ seatName(view, seat.id) }}</span>
          <span class="seat-faction"
            >{{ factions[generalById.get(seat.hero)?.faction ?? ''] ?? '' }} · 手牌
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
    <div class="classic-center">
      <p class="classic-turn">
        {{
          view.turn?.seat === view.humanSeat
            ? '你的回合'
            : `${seatName(view, view.turn?.seat)}的回合`
        }}
        · {{ phases[view.turn?.phase ?? ''] ?? view.turn?.phase }}
      </p>
      <h1>{{ prompt }}</h1>
      <div v-if="resolving.length" class="classic-revealed" aria-label="公开结算牌">
        <span>结算区</span
        ><span v-for="card in resolving" :key="card.id"
          >{{ card.name }} {{ card.rank }}</span
        >
      </div>
      <div v-if="saveError" class="classic-save-error">
        战绩保存失败 <button @click="emit('retrySave')">重试</button>
      </div>
      <button v-if="view.winner || view.drawn" class="primary-button" @click="emit('restart')">
        再来一局
      </button>
    </div>
    <div class="classic-bottom">
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
          <img :src="heroPortrait(me.hero)" :alt="`${seatName(view, me.id)}立绘`" />
          <span>{{ seatName(view, me.id) }} · {{ me.role ? roles[me.role] : '身份未知' }}</span>
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
          :judges="seatCards(me, 'judges')"
          :expansions="me.expansions?.map((item) => ({ ...item, card: faceOf(view, item.card) }))"
        />
      </div>
      <div class="classic-hand-zone">
        <details class="classic-skills">
          <summary>武将技能与场上牌</summary>
          <p v-for="seat in view.seats" :key="seat.id">
            {{ seatName(view, seat.id) }}：{{
              generalById
                .get(seat.hero)
                ?.skills.map((skill) => `${skill.name}：${skill.description}`)
                .join(' ')
            }}
          </p>
        </details>
        <div class="classic-hand-heading">
          <span>手牌 {{ hand.length }}</span
          ><span>{{
            selectedCards.length
              ? `已选 ${selectedCards.length} 张`
              : selectedHand
                ? selectedHand.description
                : '选择可用手牌'
          }}</span>
        </div>
        <div class="classic-hand" data-hand>
          <button
            v-for="card in hand"
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
            :aria-label="[card.name, card.rank, card.description].filter(Boolean).join(' ')"
            @click="chooseHand(card.id)"
          >
            <CardFace :card="card" />
          </button>
        </div>
      </div>
      <div class="classic-controls" aria-label="对局操作">
        <div v-if="equipmentButtons" class="classic-pool" aria-label="选择装备登记">
          <button
            v-for="option in equipmentButtons.options"
            :key="option.id"
            type="button"
            :data-equipment-button="option.id"
            :class="{ selected: selectedEquipmentButtons.includes(option.id) }"
            :aria-pressed="selectedEquipmentButtons.includes(option.id)"
            :disabled="!active"
            @click="selectedEquipmentButtons = toggle(selectedEquipmentButtons, option.id, equipmentButtons.required)"
          >
            {{ option.card === undefined ? cardName(option.kind) : printedFace(faceOf(view, { id: option.card, kind: option.kind })) }}
            <span v-if="option.card === undefined">（效果仍生效）</span>
          </button>
          <span>已选 {{ selectedEquipmentButtons.length }}/{{ equipmentButtons.required }}</span>
        </div>
        <div v-if="branches.length && !privateBid" class="classic-options">
          <button
            v-for="item in branches"
            :key="item.id"
            :class="{ selected: selectedMode === 'choice' && selectedBranch === item.id }"
            :disabled="!active"
            @click="setMode('choice', item.id)"
          >
            {{ item.label }}
          </button>
        </div>
        <div v-if="!choice && (virtuals.length || skills.length || activePlays.length)" class="classic-options">
          <button
            v-for="(item, i) in virtuals"
            :key="`v${i}`"
            :class="{ selected: selectedMode === 'virtual' && selectedPlay === i }"
            :disabled="!active"
            @click="setMode('virtual', i)"
          >
            {{ item.branch.label }}</button
          ><button
            v-for="(item, i) in skills"
            :key="`s${i}`"
            :class="{ selected: selectedMode === 'skill' && selectedPlay === i }"
            :disabled="!active"
            @click="setMode('skill', i)"
          >
            {{ item.branch.label }}
          </button><button
            v-for="(item, i) in activePlays"
            :key="`a${item.registration}`"
            :class="{ selected: selectedMode === 'active' && selectedPlay === i }"
            :disabled="!active"
            @click="setMode('active', i)"
          >
            {{ item.branch.label === item.skill ? skillName(item.skill) : item.branch.label }}
          </button>
        </div>
        <div v-if="pool.length" class="classic-pool">
          <button
            v-for="card in pool"
            :key="card.id"
            :data-card-id="card.id"
            :class="{ selected: selectedCards.includes(card.id) }"
            :aria-pressed="selectedCards.includes(card.id)"
            @click="choosePool(card.id)"
          >
            {{ card.name }} {{ card.rank }}
          </button>
        </div>
        <p v-if="branch && !privateBid">
          选牌 {{ selectedCards.length }}/{{ branch.minCards }}–{{ branch.maxCards }} · 目标
          {{ selectedTargets.length }}/{{ branch.minTargets }}–{{ branch.maxTargets }}
        </p>
        <div class="classic-control-row">
          <button
            v-if="choice?.kind === 'shan' || choice?.kind === 'save'"
            class="secondary"
            :disabled="!active"
            @click="pass"
          >
            放弃</button
          ><button
            v-if="!choice && view.actions.some((a) => a.type === 'endPlay')"
            class="secondary whitespace-nowrap"
            :disabled="!active"
            @click="emit('act', { type: 'endPlay' }, view.version)"
          >
            结束出牌</button
          ><button
            v-if="selectedRecast"
            :class="{ secondary: !!exactAction }"
            :disabled="!active"
            @click="recast"
          >
            重铸</button
          ><button
            v-if="!selectedRecast || currentActions.length"
            :disabled="!canConfirm"
            @click="confirm"
          >
            确认
          </button>
        </div>
      </div>
    </div>
  </section>
  <Teleport to="body">
    <MoveChoice
      v-if="moveChoice"
      :key="choice?.event"
      :request="moveChoice"
      :cards="moveChoiceCards"
      :disabled="!inputReady"
      :error="error"
      @confirm="confirmMove"
    />
  </Teleport>
</template>
