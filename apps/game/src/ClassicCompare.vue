<script setup lang="ts">
import { onBeforeUnmount, ref, shallowRef, watch } from 'vue'
import CardFace from './CardFace.vue'
import { publicComparison, type PublicComparison, type TableView } from './classicProtocol'

const props = defineProps<{ view: TableView }>()
const comparisons = shallowRef<PublicComparison[]>([])
const open = ref(false)
let timer: ReturnType<typeof setTimeout>

watch(
  () => props.view,
  (view) => {
    const announced = new Map<number, PublicComparison>()
    for (const event of view.events) {
      const published = publicComparison(view, event)
      if (published) announced.set(published.event, published)
    }
    if (!announced.size) return
    comparisons.value = [...announced.values()]
    open.value = true
    clearTimeout(timer)
    timer = setTimeout(() => {
      open.value = false
    }, 3200)
  },
  { immediate: true },
)

function toggle() {
  clearTimeout(timer)
  open.value = !open.value
}
onBeforeUnmount(() => clearTimeout(timer))
</script>

<template>
  <div v-if="comparisons.length" class="compare-disclosure" @keydown.esc="open = false">
    <button
      type="button"
      class="compare-toggle"
      aria-controls="compare-panel"
      :aria-expanded="open"
      @click="toggle"
    >
      拼点牌
    </button>
    <div v-show="open" id="compare-panel" class="compare-panel" aria-live="polite">
      <section
        v-for="comparison in comparisons"
        :key="comparison.event"
        :data-comparison="comparison.event"
      >
        <h2>{{ comparison.outcome ?? '拼点亮牌' }}</h2>
        <div class="compare-bids">
          <figure v-for="bid in comparison.bids" :key="bid.seat" :data-compare-seat="bid.seat">
            <div
              class="playing-card compare-card"
              :class="{
                red: bid.card.suit === 'heart' || bid.card.suit === 'diamond',
                'long-name': bid.card.name.length > 2,
              }"
              :data-compare-bid="bid.card.id"
              role="img"
              :aria-label="bid.face"
            >
              <CardFace :card="bid.card" />
            </div>
            <figcaption>
              {{ bid.name }}<b v-if="bid.points != null"> · {{ bid.points }}点</b>
            </figcaption>
          </figure>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.compare-disclosure {
  position: relative;
  flex: none;
  z-index: 9;
}
.compare-toggle {
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-shadow: inherit;
}
.compare-panel {
  position: absolute;
  inset: calc(100% + 4px) 0 auto auto;
  width: max-content;
  max-height: 65dvh;
  overflow: auto;
  padding: 8px 12px;
  color: #38271c;
  text-align: center;
  text-shadow: none;
  background: #f1e5cf;
  border: 1px solid #a88966;
  box-shadow: 0 3px 9px #291d1540;
}
.compare-panel h2 {
  margin: 0 0 5px;
  font-size: 15px;
}
.compare-panel section + section {
  margin-top: 8px;
}
.compare-bids {
  display: flex;
  justify-content: center;
  gap: 12px;
}
.compare-bids figure {
  display: grid;
  justify-items: center;
  gap: 3px;
  margin: 0;
}
.compare-card {
  width: clamp(49px, 9dvh, 76px);
  height: clamp(66px, 13dvh, 104px);
  min-height: 0;
  padding: 4px 6px;
  line-height: 1.05;
  transform: none;
}
.compare-card :deep(strong) {
  margin: 2px 0 0;
  font-size: clamp(15px, 2.6dvh, 22px);
}
.compare-card.long-name :deep(strong) {
  font-size: clamp(11px, 2dvh, 16px);
}
.compare-bids figcaption {
  font-size: 12px;
  white-space: nowrap;
}
</style>
