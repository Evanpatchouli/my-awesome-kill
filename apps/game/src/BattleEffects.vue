<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { gsap } from 'gsap'
import CardFace from './CardFace.vue'
import drawBack from './assets/standard/table/draw-back.png'
import { motionStep } from './battleMotion'
import type { FeedbackEvent } from './battleMotion'
type MatchView = { revision: number; events: FeedbackEvent[] }

const props = defineProps<{ view: MatchView }>()
const layer = ref<HTMLElement>()
let effects: gsap.Context | undefined
const labels = {
  play: '使用',
  respond: '响应',
  discard: '弃置',
  equip: '装备',
  judge: '判定',
  heal: '回复',
  damage: '伤害',
  draw: '摸牌',
}
const cues = computed(() => props.view.events)
function present(view: MatchView) {
  effects?.revert()
  const root = layer.value
  if (!root || localStorage.getItem('mak-reduced-motion') === 'true') return
  const board = root.parentElement
  if (!board || !view.events.length) return
  const rect = root.getBoundingClientRect()
  effects = gsap.context(() => {
    const sequence = gsap.timeline()
    const nodes = Array.from(root.children)
    view.events.forEach((event, i) => {
      const el = nodes[i] as HTMLElement | undefined
      const seat = board.querySelector<HTMLElement>(`[data-seat="${event.seat}"]`)
      if (!el || !seat) return
      const position = seat.getBoundingClientRect()
      const x = position.left + position.width / 2 - rect.left
      const y = position.top + position.height / 2 - rect.top
      const at = i * motionStep(view.events)
      const hasCards = 'cards' in event
      const tableCard = hasCards && event.cards.length === 1 && (event.type === 'play' || event.type === 'respond')
        ? board.querySelector(`[data-resolving-card="${event.cards[0]!.id}"]`)?.getBoundingClientRect()
        : undefined
      const middle = { x: rect.width * 0.5, y: rect.height * 0.51 }
      const destination = tableCard
        ? { x: tableCard.left + tableCard.width / 2 - rect.left, y: tableCard.top + tableCard.height / 2 - rect.top }
        : hasCards && event.type !== 'equip' ? middle : { x, y }
      const origin = event.type === 'draw' || event.type === 'judge' ? middle : { x, y }
      sequence.fromTo(
        el,
        { ...origin, xPercent: -50, yPercent: -50, opacity: 0, scale: hasCards || event.type === 'draw' ? 1 : 0.7 },
        {
          ...(event.type === 'draw' ? middle : destination),
          scale: 1,
          opacity: 1,
          duration: 0.23,
          ease: 'power2.out',
          immediateRender: false,
        },
        at,
      )
      if (tableCard)
        sequence.set(el, { opacity: 0 }, at + 0.23)
      else if (event.type === 'draw')
        sequence.to(el, { ...destination, opacity: 0, duration: 0.35, ease: 'power2.in' }, at + 0.28)
      else
        sequence.to(el, { opacity: 0, y: destination.y - 17, duration: 0.18 }, at + 0.44)
      if (event.type === 'damage') {
        const portrait = seat.querySelector('img') ?? seat
        sequence.fromTo(
          portrait,
          { x: -5, filter: 'sepia(1) saturate(5)' },
          { x: 0, filter: 'none', duration: 0.07, repeat: 3, yoyo: true, immediateRender: false },
          at,
        )
        sequence.set(portrait, { clearProps: 'transform,filter' }, at + 0.29)
      }
    })
  }, board)
}
onMounted(() => present(props.view))
watch(() => props.view, present, { flush: 'post' })
onBeforeUnmount(() => effects?.revert())
</script>

<template>
  <div ref="layer" class="effect-layer" aria-hidden="true">
    <div
      v-for="(event, i) in cues"
      :key="`${view.revision}-${i}`"
      class="effect-cue"
      :data-effect="event.type"
    >
      <template v-if="'cards' in event">
        <div
          v-for="card in event.cards.slice(0, 4)"
          :key="card.id"
          class="playing-card"
          :class="{ red: card.suit === 'heart' || card.suit === 'diamond', 'long-name': card.name.length > 2 }"
          :data-effect-card="card.id"
        >
          <CardFace :card="card" />
        </div>
        <span
          >{{ labels[event.type] }} ·
          {{ event.cards.length === 1 ? event.cards[0]?.name : event.cards.length + ' 张' }}</span
        >
      </template>
      <template v-else-if="event.type === 'draw'">
        <div v-for="n in Math.min(event.amount, 3)" :key="n" class="playing-card card-back" data-effect-card>
          <img :src="drawBack" alt="" />
        </div>
        <span>{{ labels.draw }} · {{ event.amount }} 张</span>
      </template>
      <strong v-else>{{ event.type === 'heal' ? '+' : '−' }}{{ event.amount }}</strong>
    </div>
  </div>
</template>

<style scoped>
.effect-layer {
  position: absolute;
  inset: 0;
  z-index: 8;
  overflow: hidden;
  pointer-events: none;
}
.effect-cue {
  position: absolute;
  top: 0;
  left: 0;
  opacity: 0;
  display: flex;
  justify-content: center;
  align-items: center;
}
.effect-cue > .playing-card + .playing-card {
  margin-left: calc(-0.2 * var(--card-width));
}
[data-effect='draw'] {
  gap: 5px;
  padding: 22px 9px 8px;
  border: 1px solid #bd9760;
  background: #231b16f5;
}
[data-effect='draw'] > .playing-card + .playing-card { margin-left: 0; }
[data-effect='draw'] > span {
  top: 2px;
  background: transparent;
  color: #f4dfb7;
  font-size: 11px;
}
.effect-cue > :is(span, strong) {
  color: #fff3d9;
  background: #523b2e;
  border-radius: 3px;
  padding: 2px 8px;
  font-weight: 700;
}
.effect-cue > span {
  position: absolute;
  top: 100%;
  font-size: 14px;
  white-space: nowrap;
}
.effect-cue > strong {
  font-size: clamp(22px, 3vw, 39px);
  line-height: 1.2;
}
[data-effect='damage'] > strong {
  background: #983628;
}
[data-effect='heal'] > strong {
  background: #47653d;
}
</style>
