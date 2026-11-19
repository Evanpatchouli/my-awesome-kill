<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ hp: number; maxHp: number }>()
const numeric = computed(() => props.hp < 0 || props.maxHp <= 0 || props.maxHp > 8)
const condition = computed(() => {
  if (props.hp <= 0) return 'low'
  if (props.hp > Math.round(props.maxHp / 2) || props.hp === props.maxHp) return 'high'
  return props.hp > Math.floor(props.maxHp / 3) ? 'mid' : 'low'
})
</script>
<template>
  <span class="health" :class="condition" role="img" :aria-label="`体力 ${hp} / ${maxHp}`">
    <span v-if="numeric" class="health-count">{{ hp }} / {{ maxHp }}</span>
    <template v-else>
      <span v-for="n in maxHp" :key="n" class="health-point" :class="{ lost: n > hp }" aria-hidden="true"></span>
    </template>
  </span>
</template>
<style scoped>
.health .health-count { color: inherit; font-size: 0.8em; font-variant-numeric: tabular-nums; letter-spacing: 0; white-space: nowrap; }
</style>
