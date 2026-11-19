<script setup lang="ts">
import type { CardPrint } from './classicProtocol'
import { cardImage } from './battleArt'

defineProps<{ card: CardPrint }>()
const suits = { spade: '♠', heart: '♥', club: '♣', diamond: '♦' }
function rankLabel(rank: number) {
  return ({ 1: 'A', 11: 'J', 12: 'Q', 13: 'K' } as Record<number, string>)[rank] ?? rank
}
</script>
<template>
  <span v-if="card.rank != null && card.suit" class="card-rank" aria-hidden="true"
    >{{ rankLabel(card.rank) }}{{ suits[card.suit] }}</span
  >
  <strong aria-hidden="true">{{ card.name }}</strong>
  <img v-if="cardImage(card.kind)" class="card-art" :src="cardImage(card.kind)" alt="" />
</template>
