<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { gsap } from 'gsap'
import { cardImage } from './battleArt'
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
      const cards = 'cards' in event
      const middle = { x: rect.width * 0.5, y: rect.height * 0.51 }
      const destination = cards && event.type !== 'equip' ? middle : { x, y }
      const origin = event.type === 'draw' || event.type === 'judge' ? middle : { x, y }
      sequence.fromTo(
        el,
        { ...origin, xPercent: -50, yPercent: -50, opacity: 0, scale: 0.7 },
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
      if (event.type === 'draw')
        sequence.to(el, { ...destination, scale: 0.45, opacity: 0, duration: 0.35, ease: 'power2.in' }, at + 0.28)
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
        <img
          v-for="card in event.cards.filter(card => cardImage(card.kind)).slice(0, 4)"
          :key="card.id"
          :src="cardImage(card.kind)"
          alt=""
        />
        <span
          >{{ labels[event.type] }} ·
          {{ event.cards.length === 1 ? event.cards[0]?.name : event.cards.length + ' 张' }}</span
        >
      </template>
      <template v-else-if="event.type === 'draw'">
        <img v-for="n in Math.min(event.amount, 3)" :key="n" :src="drawBack" alt="" />
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
  white-space: nowrap;
}
.effect-cue img {
  width: clamp(46px, 6vw, 76px);
  height: clamp(63px, 8.3vw, 104px);
  object-fit: contain;
  background: #f5e8d2;
  border: 1px solid #564234;
  border-radius: 5px;
  box-shadow: 0 3px 9px #3726154a;
}
.effect-cue img + img {
  margin-left: -22px;
}
[data-effect='draw'] {
  gap: 5px;
  padding: 22px 9px 8px;
  border: 1px solid #bd9760;
  background: #231b16f5;
}
[data-effect='draw'] img + img { margin-left: 0; }
[data-effect='draw'] span {
  top: 2px;
  background: transparent;
  color: #f4dfb7;
  font-size: 11px;
}
.effect-cue span,
.effect-cue strong {
  color: #fff3d9;
  background: #523b2e;
  border-radius: 3px;
  padding: 2px 8px;
  font-weight: 700;
}
.effect-cue span {
  position: absolute;
  top: 100%;
  font-size: 14px;
}
.effect-cue strong {
  font-size: clamp(22px, 3vw, 39px);
  line-height: 1.2;
}
[data-effect='damage'] strong {
  background: #983628;
}
[data-effect='heal'] strong {
  background: #47653d;
}
</style>
