<script setup lang="ts">
import { computed } from 'vue'
import { slot } from '@mak/rules'
import type { SeatView, TableCard } from './classicProtocol'
const props = defineProps<{
  seat: SeatView
  equipment: TableCard[]
  judges: TableCard[]
  knownHand?: TableCard[]
  label: string
}>()
const suits = { spade: '♠', heart: '♥', club: '♣', diamond: '♦' }
const labels = { weapon: '武', armor: '防', plus: '+马', minus: '−马' }
function face(card: TableCard) {
  if (!card.suit || card.rank == null) return card.name
  const rank =
    ({ 1: 'A', 11: 'J', 12: 'Q', 13: 'K' } as Record<number, string>)[card.rank] ?? card.rank
  return `${suits[card.suit]}${rank} ${card.name}`
}
const state = computed(() =>
  [props.seat.linked && '横置', props.seat.turnedOver && '已翻面', props.seat.armor && `护甲 ${props.seat.armor}`]
    .filter(Boolean)
    .join(' · '),
)
const equipped = computed(() =>
  props.equipment.map((card) => `${labels[slot(card.kind)!] ?? '装备'} ${face(card)}`).join(' · '),
)
const judgment = computed(() => props.judges.map(face).join(' · '))
const known = computed(() => props.knownHand?.map(face).join(' · '))
</script>
<template>
  <div class="classic-zones" :aria-label="label + '的场上信息'">
    <div v-if="state" class="classic-seat-state" :aria-label="label + '的状态'" :title="state">
      {{ state }}
    </div>
    <div :title="equipped || '无装备'"><span>装备</span> {{ equipped || '—' }}</div>
    <div :title="judgment || '无判定牌'"><span>判定</span> {{ judgment || '—' }}</div>
    <div v-if="known" :title="known"><span>已知手牌</span> {{ known }}</div>
  </div>
</template>
