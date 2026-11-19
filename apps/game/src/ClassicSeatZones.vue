<script setup lang="ts">
import { computed } from 'vue'
import { slot } from '@mak/rules'
import { printedFace, skillName, type SeatView, type TableCard } from './classicProtocol'
import CardFace from './CardFace.vue'
const props = defineProps<{
  seat: SeatView
  equipment: TableCard[]
  judges: TableCard[]
  knownHand?: TableCard[]
  expansions?: { card: TableCard; tags: string[] }[]
  label: string
  seatNames: string[]
}>()
const labels = { weapon: '武', armor: '防', plus: '+马', minus: '−马' }
const absence = computed(() => {
  const clock = props.seat.rest
  if (!clock) return props.seat.out ? '已离场' : ''
  const label = props.seat.out ? '休整' : '休整标记'
  if (clock.count <= 0) return `${label} · 无限期`
  return `${label} · 余 ${clock.count} ${clock.unit === 'round' ? '轮' : '回合'}`
})
const state = computed(() =>
  [absence.value, props.seat.linked && '横置', props.seat.turnedOver && '已翻面', props.seat.armor && `护甲 ${props.seat.armor}`]
    .filter(Boolean)
    .join(' · '),
)
const equipped = computed(() =>
  props.equipment.map((card) => `${labels[slot(card.kind)!] ?? '装备'} ${printedFace(card)}`).join(' · '),
)
const judgment = computed(() => props.judges.map(printedFace).join(' · '))
const known = computed(() => props.knownHand?.map(printedFace).join(' · '))
</script>
<template>
  <div class="classic-zones" :aria-label="label + '的场上信息'">
    <div v-if="state" class="classic-seat-state" :data-rest-seat="seat.rest ? seat.id : undefined" :aria-label="label + '的状态'" :title="state">
      {{ state }}
    </div>
    <div :title="equipped || '无装备'"><span>装备</span> {{ equipped || '—' }}</div>
    <div :title="judgment || '无判定牌'"><span>判定</span> {{ judgment || '—' }}</div>
    <div v-if="known" :title="known"><span>已知手牌</span> {{ known }}</div>
    <div v-for="mark in seat.marks ?? []" :key="`${mark.skill}-${mark.source}`" class="classic-seat-mark" :data-mark="mark.skill">
      {{ skillName(mark.skill) }} ×{{ mark.count }}<template v-if="mark.source != null"> · 来源 {{ seatNames[mark.source] }}</template>
    </div>
    <div v-if="seat.limitedSpent?.length" class="classic-limited">限定已用：{{ seat.limitedSpent.map(skillName).join('、') }}</div>
    <details v-if="expansions?.length" class="classic-expansions">
      <summary>扩展牌 {{ expansions.length }}</summary>
      <div class="classic-expansion-list">
        <div v-for="item in expansions" :key="item.card.id" :data-expansion-card="item.card.id">
          <div class="playing-card"><CardFace :card="item.card" /></div>
          <span>{{ item.tags.map((tag) => `${skillName(tag)} (${tag})`).join('、') }}</span>
        </div>
      </div>
    </details>
  </div>
</template>
