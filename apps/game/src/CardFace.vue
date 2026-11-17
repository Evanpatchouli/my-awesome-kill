<script setup lang="ts">
import type { Card } from '@mak/rules'
import { cardImage } from './battleArt'

defineProps<{ card: Card }>()
const suits = { spade: '♠', heart: '♥', club: '♣', diamond: '♦' }
function rankLabel(rank: number) {
  return ({ 1: 'A', 11: 'J', 12: 'Q', 13: 'K' } as Record<number, string>)[rank] ?? rank
}
</script>
<template>
  <span class="card-rank" aria-hidden="true">{{ rankLabel(card.rank) }}{{ suits[card.suit] }}</span>
  <strong aria-hidden="true">{{ card.name }}</strong>
  <img class="card-art" :src="cardImage(card.kind)" alt="" />
</template>
