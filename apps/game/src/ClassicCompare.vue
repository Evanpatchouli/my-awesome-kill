<script setup lang="ts">
import { onBeforeUnmount, ref, shallowRef, watch } from 'vue'
import CardFace from './CardFace.vue'
import { publicComparison, updateGroupComparison, type PublicComparison, type TableView } from './classicProtocol'

const props = defineProps<{ view: TableView }>()
const comparisons = shallowRef<PublicComparison[]>([])
const activeGroups = new Map<number, PublicComparison[]>()
const open = ref(false)
let timer: ReturnType<typeof setTimeout>

watch(
  () => props.view,
  (view, prior) => {
    if (view.version === 0 && prior) {
      comparisons.value = []
      activeGroups.clear()
      open.value = false
    }
    const announced = new Map<number, PublicComparison>()
    for (const event of view.events) {
      const published = publicComparison(view, event)
      if (published) announced.set(published.event, published)
    }
    if (!announced.size && !view.groupUpdates?.length) return
    const groups = new Map<number, PublicComparison[]>()
    for (const update of view.groupUpdates ?? []) {
      const rows = updateGroupComparison(view, activeGroups.get(update.comparison) ?? [], update)
      groups.set(update.comparison, rows)
      if (update.type === 'result') activeGroups.delete(update.comparison)
      else activeGroups.set(update.comparison, rows)
    }
    comparisons.value = [...announced.values(), ...[...groups.values()].flat()]
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
        :key="`${comparison.event}-${comparison.target ?? 'table'}`"
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
        <ul v-if="comparison.rounds" class="compare-rounds">
          <li v-for="(round, index) in comparison.rounds" :key="index">对{{ round.target }} · {{ round.first }} 比 {{ round.second }}</li>
        </ul>
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
.compare-toggle:hover { color: var(--battle-selected); }
.compare-panel {
  position: absolute;
  inset: calc(100% + 4px) 0 auto auto;
  width: max-content;
  max-width: min(83vw, 38rem);
  max-height: 65dvh;
  overflow: auto;
  padding: 8px 12px;
  color: var(--battle-paper);
  text-align: center;
  text-shadow: none;
  background: linear-gradient(#1b231fe0, #1b231fe0), var(--battle-lacquer) center / 540px;
  border: 1px solid var(--battle-copper);
  box-shadow: 0 8px 22px #1118159c;
}
.compare-panel h2 {
  margin: 0 0 5px;
  font-size: 15px;
}
.compare-panel section + section {
  margin-top: 8px;
}
.compare-rounds {
  list-style: none;
  padding: 0;
  margin: 6px 0 0;
}
.compare-bids {
  display: flex;
  flex-wrap: wrap;
  justify-content: safe center;
  gap: 12px;
}
.compare-bids figure {
  display: grid;
  justify-items: center;
  gap: 3px;
  margin: 0;
}
.compare-card {
  transform: none;
}
.compare-bids figcaption {
  font-size: 12px;
  white-space: nowrap;
}
</style>
