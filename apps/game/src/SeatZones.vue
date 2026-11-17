<script setup lang="ts">
import { computed } from 'vue'
import { slot, type Card } from '@mak/rules'

const props = defineProps<{ equipment: Card[]; judges: Card[]; label: string }>()
const suits = { spade: '♠', heart: '♥', club: '♣', diamond: '♦' }
const slots = [
  { label: '武器', type: 'weapon' },
  { label: '防具', type: 'armor' },
  { label: '+马', type: 'plus' },
  { label: '−马', type: 'minus' },
] as const
const equipmentSlots = computed(() =>
  slots.map((item) => ({
    ...item,
    card: props.equipment.find((card) => slot(card.kind) === item.type),
  })),
)
function face(card: Card) {
  const rank = ({ 1: 'A', 11: 'J', 12: 'Q', 13: 'K' } as Record<number, string>)[card.rank] ?? card.rank
  return `${suits[card.suit]}${rank} ${card.name}`
}
</script>
<template>
  <section class="public-zone" :aria-label="label + '的装备与判定区'">
    <div
      v-for="item in equipmentSlots"
      :key="item.label"
      class="public-slot"
      :class="{ red: item.card && (item.card.suit === 'heart' || item.card.suit === 'diamond') }"
      :title="item.card?.description"
    >
      <span>{{ item.label }}</span><strong>{{ item.card ? face(item.card) : '—' }}</strong>
    </div>
    <div class="public-slot judge-slot">
      <span>判定</span><strong v-if="!judges.length">—</strong>
      <strong
        v-for="card in judges"
        :key="card.id"
        :class="{ red: card.suit === 'heart' || card.suit === 'diamond' }"
        :title="card.description"
      >{{ face(card) }}</strong>
    </div>
  </section>
</template>
