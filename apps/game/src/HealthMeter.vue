<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ hp: number; maxHp: number }>()
const condition = computed(() =>
  props.hp > Math.round(props.maxHp / 2) || props.hp === props.maxHp
    ? 'high'
    : props.hp > Math.floor(props.maxHp / 3)
      ? 'mid'
      : 'low',
)
</script>
<template>
  <p class="health" :class="condition" :aria-label="`体力 ${Math.max(0, hp)} / ${maxHp}`">
    <span
      v-for="n in Math.max(0, maxHp)"
      :key="n"
      class="health-point"
      :class="{ lost: n > hp }"
      aria-hidden="true"
    ></span>
  </p>
</template>
