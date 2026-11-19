<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import CardFace from './CardFace.vue'
import { printedFace } from './classicProtocol'
import type { MoveChoiceView, MoveStep, TableCard } from './classicProtocol'

const props = defineProps<{
  request: MoveChoiceView
  cards: TableCard[]
  disabled: boolean
  error: string
  controlsTarget: HTMLElement
}>()
const emit = defineEmits<{ confirm: [moves: MoveStep[] | null] }>()

const rows = ref<number[][]>([])
const moves = ref<MoveStep[]>([])
const priorRows = ref<number[][][]>([])
const positions = ref<Record<number, number>>({})
const selected = ref<number | null>(null)
const cardArea = ref<HTMLElement | null>(null)
onMounted(() => cardArea.value?.focus({ preventScroll: true }))
watch(() => props.request, (request) => {
  rows.value = request.rows.map((row) => [...row.cards])
  moves.value = []
  priorRows.value = []
  positions.value = {}
  selected.value = null
}, { immediate: true })

const cardById = computed(() => new Map(props.cards.map((card) => [card.id, card])))
const selectedRow = computed(() => rows.value.findIndex((row) => selected.value !== null && row.includes(selected.value)))
const valid = computed(() => rows.value.every((row, index) => {
  const rule = props.request.rows[index]
  return !!rule && row.length >= rule.min && row.length <= rule.max
}))
function cardName(id: number) {
  const card = cardById.value.get(id)
  return card ? printedFace(card) : '未知牌'
}
function move(card: number, target: number, index: number) {
  if (props.disabled) return
  const source = rows.value.findIndex((row) => row.includes(card))
  const sourceRow = rows.value[source]
  const targetRow = rows.value[target]
  if (!sourceRow || !targetRow || (source !== target && targetRow.length >= props.request.rows[target]!.max)) return
  const oldIndex = sourceRow.indexOf(card)
  if (source === target && oldIndex === index) return
  if (index < 0 || index > targetRow.length - Number(source === target)) return
  priorRows.value.push(rows.value.map((row) => [...row]))
  sourceRow.splice(oldIndex, 1)
  targetRow.splice(index, 0, card)
  moves.value.push({ card, row: target, index })
  positions.value = {}
}
function undo() {
  const previous = priorRows.value.pop()
  if (!previous) return
  rows.value = previous
  moves.value.pop()
  positions.value = {}
}
function possiblePredecessors(target: number) {
  return (rows.value[target] ?? []).filter((card) => card !== selected.value)
}
function insertionIndex(target: number) {
  const prefer = positions.value[target]
  return prefer != null && prefer >= 0 ? prefer : possiblePredecessors(target).length
}
function canMoveSelected(target: number) {
  if (props.disabled || selected.value === null) return false
  const members = rows.value[target] ?? []
  return selectedRow.value === target
    ? members.indexOf(selected.value) !== insertionIndex(target)
    : members.length < props.request.rows[target]!.max
}
function moveSelected(target: number) {
  if (selected.value !== null && canMoveSelected(target)) {
    move(selected.value, target, insertionIndex(target))
  }
}
function selectCard(id: number) {
  selected.value = selected.value === id ? null : id
  positions.value = {}
}
function selectPosition(row: number, event: Event) {
  if (event.target instanceof HTMLSelectElement) positions.value[row] = Number(event.target.value)
}
function confirm() {
  if (!props.disabled && valid.value) emit('confirm', moves.value.map((step) => ({ ...step })))
}
</script>

<template>
  <section ref="cardArea" class="move-choice" :aria-label="request.prompt" tabindex="-1" data-move-choice>
    <h2>{{ request.prompt }}</h2>
    <div class="move-choice-rows">
      <section v-for="(row, rowIndex) in request.rows" :key="rowIndex" class="move-choice-row" :data-row="rowIndex" :aria-label="row.label">
        <div class="move-choice-row-heading">
          <strong>{{ row.label }}</strong>
          <span>{{ rows[rowIndex]?.length ?? 0 }} / {{ row.min }}–{{ row.max }}</span>
        </div>
        <div class="move-choice-cards">
          <button
            v-for="id in rows[rowIndex]"
            :key="id"
            type="button"
            class="playing-card move-choice-card"
            :class="{
              red: cardById.get(id)?.suit === 'heart' || cardById.get(id)?.suit === 'diamond',
              'long-name': (cardById.get(id)?.name.length ?? 0) > 2,
              selected: selected === id,
            }"
            :data-card-id="id"
            :disabled="disabled"
            :aria-pressed="selected === id"
            :aria-label="`选择${cardName(id)}`"
            @click="selectCard(id)"
          >
            <CardFace v-if="cardById.get(id)" :card="cardById.get(id)!" />
            <strong v-else>未知牌</strong>
          </button>
          <span v-if="!rows[rowIndex]?.length" class="move-choice-empty">暂无牌</span>
        </div>
      </section>
    </div>
  </section>
  <Teleport :to="controlsTarget">
    <div class="move-choice-controls" role="group" aria-label="牌堆整理操作" data-move-controls>
      <div v-if="selected !== null" class="move-choice-destinations">
        <div v-for="(row, rowIndex) in request.rows" :key="rowIndex" class="move-choice-destination">
          <select :value="positions[rowIndex] ?? -1" :disabled="disabled" :aria-label="`${row.label}插入位置`" @change="selectPosition(rowIndex, $event)">
            <option :value="-1">末尾</option>
            <option v-for="(id, i) in possiblePredecessors(rowIndex)" :key="id" :value="i">第 {{ i + 1 }} 张前 · {{ cardName(id) }}</option>
          </select>
          <button type="button" class="secondary" :disabled="!canMoveSelected(rowIndex)" @click="moveSelected(rowIndex)">移动到{{ row.label }}</button>
        </div>
      </div>
      <div class="move-choice-actions">
        <span v-if="!valid" class="move-choice-hint">请调整各区域牌数</span>
        <p v-if="error" role="alert" class="move-choice-error">{{ error }}</p>
        <button type="button" class="secondary" :disabled="disabled || !moves.length" @click="undo">撤销移动</button>
        <button v-if="request.allowDecline" type="button" class="secondary" :disabled="disabled" @click="emit('confirm', null)">放弃</button>
        <button type="button" :disabled="disabled || !valid" @click="confirm">确认</button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.move-choice { color: #f4dfb7; }
.move-choice h2 { margin: 0; padding-bottom: 5px; border-bottom: 1px solid #76583b; font-size: 18px; }
.move-choice-rows { display: flex; gap: 12px; }
.move-choice-row { flex: 1 1 auto; min-width: 0; padding-top: 4px; }
.move-choice-row-heading { display: flex; gap: 8px; align-items: center; font-size: 12px; }
.move-choice-row-heading strong { font-size: 14px; }
.move-choice-cards { display: flex; align-items: flex-start; gap: 6px; overflow-x: auto; padding: 5px 3px 2px; scrollbar-width: thin; }
.move-choice-card { flex: 0 0 66px; height: 94px; min-height: 0; padding: 5px; }
.move-choice-card :deep(strong) { font-size: 17px; margin: 3px 0 0; }
.move-choice-card.long-name :deep(strong) { font-size: 13px; }
.move-choice-empty { color: #d3b99c; font-size: 12px; padding: 8px 2px; }

.move-choice-controls { display: grid; gap: 4px; text-align: center; pointer-events: auto; }
.move-choice-destinations, .move-choice-actions { display: flex; align-items: center; justify-content: center; flex-wrap: wrap; gap: 4px 6px; }
.move-choice-destination { display: flex; gap: 3px; }
.move-choice-controls :is(button, select) { border: 1px solid #a4846a; background: #f1e4d2; color: #573f31; padding: 4px 8px; font-size: 12px; }
.move-choice-controls select { max-width: 125px; padding-inline: 3px; }
.move-choice-controls button { cursor: pointer; }
.move-choice-controls button:disabled { opacity: .5; cursor: default; }
.move-choice-controls :is(button, select):focus-visible { outline: 2px solid #e8b970; outline-offset: 2px; }
.move-choice-actions button { min-width: 76px; }
.move-choice-actions button:not(.secondary) { background: #903d2d; border-color: #903d2d; color: white; }
.move-choice-hint, .move-choice-error { margin: 0; color: #f4dfb7; background: #292019ef; padding: 2px 5px; font-size: 12px; }
.move-choice-error { color: #ffc5a6; }
@media (max-height: 500px) {
  .move-choice h2 { font-size: 14px; padding-bottom: 2px; }
  .move-choice-rows { gap: 8px; }
  .move-choice-row { padding-top: 2px; }
  .move-choice-row-heading, .move-choice-row-heading strong { font-size: 11px; }
  .move-choice-cards { padding: 2px 3px; }
  .move-choice-card { flex-basis: 49px; height: 62px; padding: 3px; }
  .move-choice-card :deep(strong) { font-size: 13px; margin: 1px 0 0; }
  .move-choice-card.long-name :deep(strong) { font-size: 10px; }
  .move-choice-controls :is(button, select) { padding: 2px 4px; font-size: 11px; }
  .move-choice-controls select { max-width: 86px; }
  .move-choice-actions button { min-width: 55px; }
}
@media (max-height: 350px) {
  .move-choice-card { height: 49px; }
  .move-choice-controls { display: flex; align-items: center; justify-content: center; flex-wrap: wrap; }
}
</style>
