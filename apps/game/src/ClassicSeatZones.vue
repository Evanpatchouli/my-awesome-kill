<script setup lang="ts">
import { computed } from 'vue'
import { slot } from '@mak/rules'
import { cardName, printedFace, skillName, type SeatView, type TableCard } from './classicProtocol'
import CardFace from './CardFace.vue'
const props = defineProps<{
  seat: SeatView
  equipment: TableCard[]
  judges: TableCard[]
  knownHand?: TableCard[]
  expansions?: { card: TableCard; tags: string[] }[]
  label: string
  seatNames: string[]
  mountCombined?: boolean
  hideEquipment?: boolean
}>()
const labels = { weapon: '武', armor: '防', plus: '+马', minus: '−马' }
const slotLabels = { weapon: '武器', armor: '防具', defenseHorse: '防御马', attackHorse: '进攻马', treasure: '宝物' }
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
const judgmentCards = computed(() => {
  if (!props.seat.logicalJudges?.length) return props.judges.map(printedFace)
  return props.seat.logicalJudges.map(entry => {
    const attached = props.judges.find(card => card.id === entry.material)
    return attached ? printedFace({ ...attached, name: cardName(entry.kind) }) : cardName(entry.kind)
  })
})
const judgment = computed(() => [
  ...(props.seat.judgeDisabledMarker ? ['已废除'] : []),
  ...judgmentCards.value,
].join(' · '))
const known = computed(() => props.knownHand?.map(printedFace).join(' · '))
</script>
<template>
  <div class="classic-zones" :aria-label="label + '的场上信息'">
    <div v-if="state" class="classic-seat-state" :data-rest-seat="seat.rest ? seat.id : undefined" :aria-label="label + '的状态'" :title="state">
      {{ state }}
    </div>
    <div v-if="!hideEquipment" :title="equipped || '无装备'"><span>装备</span> {{ equipped || '—' }}</div>
    <div v-if="seat.equipmentDisabledMarkers?.length" :data-disabled-equipment="seat.id" title="已废除的装备栏">
      <span>废栏</span> {{ seat.equipmentDisabledMarkers.map(slot => mountCombined && slot === 'defenseHorse' ? '坐骑栏' : slotLabels[slot]).join(' · ') }}
    </div>
    <div v-if="seat.detachedEquipment?.length" :data-equipment-effect="seat.id" title="实体已经离开，装备效果仍然生效">
      <span>生效</span> {{ seat.detachedEquipment.map(kind => cardName(kind)).join(' · ') }}
    </div>
    <div :data-judge-area="seat.id" :title="judgment || '无判定牌'">
      <span>判定</span> {{ seat.judgeDisabledMarker ? '已废除' : judgment || '—' }}
      <template v-if="seat.judgeDisabledMarker">
        <div v-for="(label, index) in judgmentCards" :key="index" class="retained-judge">{{ label }}</div>
      </template>
    </div>
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
