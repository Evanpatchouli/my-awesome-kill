<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import CardFace from './CardFace.vue'
import { printedFace } from './classicProtocol'
import type { MoveChoiceView, MoveStep, TableCard } from './classicProtocol'

const props = defineProps<{ request: MoveChoiceView; cards: TableCard[]; disabled: boolean; error: string }>()
const emit = defineEmits<{ confirm: [moves: MoveStep[] | null] }>()

const rows = ref<number[][]>([])
const moves = ref<MoveStep[]>([])
const priorRows = ref<number[][][]>([])
const positions = ref<Record<number, number>>({})
const selected = ref<number | null>(null)
const dialog = ref<HTMLElement | null>(null)
onMounted(() => dialog.value?.focus())
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
  <div class="move-choice-overlay">
    <section ref="dialog" class="move-choice" role="dialog" aria-modal="true" :aria-label="request.prompt" tabindex="-1" data-move-choice>
      <header class="move-choice-header">
        <h2>{{ request.prompt }}</h2>
        <p>选择牌后移动到其他区域，或用左右按钮调整顺序</p>
      </header>
      <div class="move-choice-rows">
        <section v-for="(row, rowIndex) in request.rows" :key="rowIndex" class="move-choice-row" :data-row="rowIndex" :aria-label="row.label">
          <div class="move-choice-row-heading">
            <strong>{{ row.label }}</strong>
            <span>{{ rows[rowIndex]?.length ?? 0 }} / {{ row.min }}–{{ row.max }}</span>
            <label v-if="selected !== null" class="move-choice-position">
              <select :value="positions[rowIndex] ?? -1" :disabled="disabled" :aria-label="`${row.label}插入位置`" @change="selectPosition(rowIndex, $event)">
                <option :value="-1">末尾</option>
                <option v-for="(id, i) in possiblePredecessors(rowIndex)" :key="id" :value="i">第 {{ i + 1 }} 张前 · {{ cardName(id) }}</option>
              </select>
            </label>
            <button type="button" :disabled="!canMoveSelected(rowIndex)" :aria-label="`移动到${row.label}`" @click="moveSelected(rowIndex)">移动到{{ row.label }}</button>
          </div>
          <div class="move-choice-cards">
            <div v-for="(id, index) in rows[rowIndex]" :key="id" class="move-choice-card-wrap">
              <button type="button" class="playing-card move-choice-card" :class="{ red: cardById.get(id)?.suit === 'heart' || cardById.get(id)?.suit === 'diamond', 'long-name': (cardById.get(id)?.name.length ?? 0) > 2, selected: selected === id }" :data-card-id="id" :disabled="disabled" :aria-pressed="selected === id" :aria-label="`选择${cardName(id)}`" @click="selectCard(id)">
                <CardFace v-if="cardById.get(id)" :card="cardById.get(id)!" />
                <strong v-else>未知牌</strong>
              </button>
              <div class="move-choice-order">
                <button type="button" :disabled="disabled || index === 0" :aria-label="`${cardName(id)}左移`" @click="move(id, rowIndex, index - 1)">←</button>
                <button type="button" :disabled="disabled || index === rows[rowIndex]!.length - 1" :aria-label="`${cardName(id)}右移`" @click="move(id, rowIndex, index + 1)">→</button>
              </div>
            </div>
            <span v-if="!rows[rowIndex]?.length" class="move-choice-empty">暂无牌</span>
          </div>
        </section>
      </div>
      <p v-if="error" role="alert" class="move-choice-error">{{ error }}</p>
      <footer class="move-choice-actions">
        <span v-if="!valid" class="move-choice-hint">请调整各区域牌数</span>
        <button type="button" class="secondary" :disabled="disabled || !moves.length" @click="undo">撤销移动</button>
        <button v-if="request.allowDecline" type="button" class="secondary" :disabled="disabled" @click="emit('confirm', null)">放弃</button>
        <button type="button" :disabled="disabled || !valid" @click="confirm">确认</button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.move-choice-overlay { position: fixed; inset: 0; z-index: 40; display: flex; align-items: center; justify-content: center; padding: 8px; background: #1b130e99; }
.move-choice { display: flex; flex-direction: column; width: min(760px, 100%); max-height: 100%; min-height: 0; color: #30271e; background: #f1e4d2; border: 2px solid #a4846a; box-shadow: 0 8px 28px #1b130e99; }
.move-choice-header { flex: none; padding: 9px 14px 5px; border-bottom: 1px solid #c6aa8b; }
.move-choice-header h2 { margin: 0; font-size: 18px; }
.move-choice-header p { margin: 3px 0 0; font-size: 12px; }
.move-choice-rows { min-height: 0; overflow: auto; padding: 4px 12px; }
.move-choice-row { padding: 5px 0; border-bottom: 1px solid #d3bda4; }
.move-choice-row-heading { display: flex; gap: 8px; align-items: center; font-size: 12px; }
.move-choice-row-heading strong { font-size: 14px; }
.move-choice-row-heading button { margin-left: auto; }
.move-choice-position { margin-left: auto; }
.move-choice-position + button { margin-left: 0; }
.move-choice-position select { max-width: 125px; border: 1px solid #a4846a; background: #f1e4d2; padding: 3px; }
.move-choice button:not(.playing-card) { border: 1px solid #a4846a; background: #f1e4d2; color: #573f31; padding: 4px 8px; font-size: 12px; cursor: pointer; }
.move-choice button:disabled { opacity: .5; cursor: default; }
.move-choice button:focus-visible { outline: 2px solid #913b2d; outline-offset: 2px; }
.move-choice-cards { display: flex; align-items: flex-start; gap: 6px; overflow-x: auto; padding: 5px 3px 2px; }
.move-choice-card-wrap { flex: 0 0 66px; }
.move-choice .move-choice-card { width: 66px; height: 94px; min-height: 0; padding: 5px; }
.move-choice .move-choice-card :deep(strong) { font-size: 17px; margin: 3px 0 0; }
.move-choice .move-choice-card.long-name :deep(strong) { font-size: 13px; }
.move-choice-order { display: flex; gap: 2px; }
.move-choice .move-choice-order button { flex: 1; padding: 2px; }
.move-choice-empty { color: #765d4a; font-size: 12px; padding: 8px 2px; }
.move-choice-actions { flex: none; display: flex; align-items: center; justify-content: flex-end; gap: 6px; padding: 7px 12px; border-top: 1px solid #c6aa8b; }
.move-choice .move-choice-actions button { min-width: 76px; background: #903d2d; border-color: #903d2d; color: white; }
.move-choice .move-choice-actions button.secondary { background: #f1e4d2; color: #513d30; border-color: #a4846a; }
.move-choice-hint { margin-right: auto; color: #903d2d; font-size: 12px; }
.move-choice-error { margin: 0; padding: 4px 12px; color: #903d2d; font-size: 12px; }
@media (max-height: 500px) {
  .move-choice-overlay { padding: 4px; }
  .move-choice-header { padding: 4px 10px; }
  .move-choice-header h2 { font-size: 14px; }
  .move-choice-header p { font-size: 11px; margin: 0; }
  .move-choice-rows { padding: 2px 10px; }
  .move-choice-row { padding: 2px 0; }
  .move-choice-cards { padding: 2px 3px; }
  .move-choice-card-wrap { flex-basis: 49px; }
  .move-choice .move-choice-card { width: 49px; height: 62px; padding: 3px; }
  .move-choice .move-choice-card :deep(strong) { font-size: 13px; margin: 1px 0 0; }
  .move-choice .move-choice-card.long-name :deep(strong) { font-size: 10px; }
  .move-choice-actions { padding: 4px 10px; }
}
@media (max-height: 315px) and (min-width: 540px) {
  .move-choice-rows { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
  .move-choice-row { min-width: 0; border: 0; padding: 4px; }
  .move-choice-row-heading { flex-wrap: wrap; gap: 4px; }
  .move-choice-position select { max-width: 103px; }
}
</style>
