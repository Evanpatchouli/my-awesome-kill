<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { gsap } from 'gsap'
import CardFace from './CardFace.vue'
import drawBack from './assets/standard/table/draw-back.png'
import { battleMotionReduced, motionStep, type FeedbackEvent } from './battleMotion'

type MatchView = { revision: number; events: FeedbackEvent[] }
type Point = { x: number; y: number }
type Timeline = gsap.core.Timeline

const props = defineProps<{ view: MatchView }>()
const layer = ref<HTMLElement>()
let effects: gsap.Context | undefined
const cardLabels = { play: '', respond: '响应', discard: '弃置', equip: '装备', judge: '判定' }

function center(el: Element, board: DOMRect): Point {
  const rect = el.getBoundingClientRect()
  return {
    x: rect.left + rect.width / 2 - board.left,
    y: rect.top + rect.height / 2 - board.top,
  }
}

function glide(sequence: Timeline, el: HTMLElement, origin: Point, destination: Point, at: number, duration: number) {
  sequence.fromTo(
    el,
    { ...origin, xPercent: -50, yPercent: -50, opacity: 0 },
    { ...destination, opacity: 1, duration, ease: 'power2.out', immediateRender: false },
    at,
  )
}

function trace(sequence: Timeline, line: HTMLElement, origin: Point, end: Point, at: number) {
  const dx = end.x - origin.x
  const dy = end.y - origin.y
  sequence.fromTo(line,
    { ...origin, width: Math.hypot(dx, dy), rotation: Math.atan2(dy, dx) * 180 / Math.PI, scaleX: 0, opacity: 0 },
    { scaleX: 1, opacity: 1, duration: 0.16, immediateRender: false, ease: 'power2.out' },
    at,
  )
  sequence.to(line, { opacity: 0, duration: 0.25 }, at + 0.18)
}

function pulse(sequence: Timeline, flash: HTMLElement, seat: Element, rect: DOMRect, at: number) {
  const frame = seat.getBoundingClientRect()
  sequence.fromTo(flash,
    { x: frame.left - rect.left, y: frame.top - rect.top, width: frame.width, height: frame.height, opacity: 0.95 },
    { opacity: 0, duration: 0.38, immediateRender: false, ease: 'power2.out' },
    at,
  )
}

function drawDestination(board: HTMLElement, seat: HTMLElement, rect: DOMRect) {
  const hand = board.querySelector<HTMLElement>('[data-hand]')
  if (!seat.classList.contains('classic-self') || !hand) return center(seat, rect)
  const handBox = hand.getBoundingClientRect()
  const lastCard = hand.querySelector('.playing-card:last-child')
  const end = center(lastCard ?? hand, rect)
  return { ...end, x: gsap.utils.clamp(handBox.left - rect.left, handBox.right - rect.left, end.x) }
}

function present(view: MatchView) {
  effects?.revert()
  effects = undefined
  const root = layer.value
  const board = root?.parentElement
  if (!root || !board || !view.events.length || battleMotionReduced()) return
  const rect = root.getBoundingClientRect()
  const middle = { x: rect.width * 0.5, y: rect.height * 0.5 }
  const stock = board.querySelector('[data-draw-anchor]')
  const stockPoint = stock ? center(stock, rect) : middle
  const step = motionStep(view.events)
  effects = gsap.context(() => {
    const sequence = gsap.timeline()
    const arrivals = new Set<Element>()
    view.events.forEach((event, index) => {
      const node = root.children[index]
      const seat = board.querySelector<HTMLElement>('[data-seat="' + event.seat + '"]')
      if (!node || !seat) return
      const portrait = seat.querySelector('.seat-select, .self-select') ?? seat
      const from = center(portrait, rect)
      const at = index * step
      const token = node.querySelector<HTMLElement>('.effect-token')
      if (!token) return

      if ('cards' in event) {
        const cardId = event.cards[0]?.id
        const tableCard = event.cards.length !== 1 || (event.type !== 'play' && event.type !== 'respond')
          ? undefined
          : event.event != null
            ? board.querySelector<HTMLElement>('[data-resolving-virtual="' + event.event + '"]')
            : cardId != null
              ? board.querySelector<HTMLElement>('[data-resolving-card="' + cardId + '"]')
              : undefined
        const destination = tableCard ? center(tableCard, rect) : event.type === 'equip' ? from : middle
        const duration = 0.29
        glide(sequence, token, event.type === 'judge' ? stockPoint : from, destination, at, duration)
        if (tableCard && !arrivals.has(tableCard)) {
          arrivals.add(tableCard)
          gsap.set(tableCard, { opacity: 0 })
          sequence.set(tableCard, { clearProps: 'opacity' }, at + duration)
          sequence.set(token, { opacity: 0 }, at + duration)
        } else {
          sequence.to(token, { opacity: 0, y: destination.y - 10, ease: 'sine.in', duration: 0.2 }, at + 0.43)
        }
        const target = event.target != null && event.target !== event.seat
          ? board.querySelector('[data-seat="' + event.target + '"]')
          : undefined
        const line = node.querySelector<HTMLElement>('.effect-trace')
        if (line && target) trace(sequence, line, from, center(target, rect), at + 0.02)
        const surge = node.querySelector<HTMLElement>('.effect-surge')
        if (surge) {
          sequence.fromTo(surge, { xPercent: -13, opacity: 0 }, {
            xPercent: 0, opacity: 0.86, duration: 0.22, immediateRender: false, ease: 'power2.out',
          }, at)
          sequence.to(surge, { xPercent: 4, opacity: 0, duration: 0.33, ease: 'power1.in' }, at + 0.26)
        }
        return
      }

      if (event.type === 'draw') {
        const destination = drawDestination(board, seat, rect)
        glide(sequence, token, stockPoint, destination, at, 0.38)
        sequence.to(token, { opacity: 0, duration: 0.14 }, at + 0.36)
        return
      }

      const flash = node.querySelector<HTMLElement>('.effect-seat-flash')
      if (flash) pulse(sequence, flash, portrait, rect, at)
      if (event.type === 'damage') {
        const art = portrait.querySelector('img') ?? portrait
        sequence.fromTo(art, { x: -5 }, {
          x: 5, duration: 0.046, repeat: 3, yoyo: true, immediateRender: false,
        }, at)
        sequence.set(art, { clearProps: 'transform' }, at + 0.185)
      }
      glide(sequence, token, from, { x: from.x, y: from.y - 9 }, at, 0.13)
      sequence.to(token, { y: from.y - 22, opacity: 0, duration: 0.25, ease: 'power1.in' }, at + 0.35)
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
      v-for="(event, i) in view.events"
      :key="view.revision + '-' + i"
      class="effect-cue"
      :data-effect="event.type"
      :data-effect-seat="event.seat"
    >
      <template v-if="'cards' in event">
        <div
          v-if="event.type === 'play' && event.cards.some(card => card.kind === 'nanman' || card.kind === 'wanjian')"
          class="effect-surge"
        ></div>
        <div v-if="event.target != null && event.target !== event.seat" class="effect-trace"></div>
        <div class="effect-token">
          <div
            v-for="(card, index) in event.cards.slice(0, 4)"
            :key="card.id ?? index"
            class="playing-card"
            :class="{ red: card.suit === 'heart' || card.suit === 'diamond', 'long-name': card.name.length > 2 }"
            :data-effect-card="card.id"
          >
            <CardFace :card="card" />
          </div>
          <span v-if="cardLabels[event.type]" class="effect-label">{{ cardLabels[event.type] }}</span>
        </div>
      </template>
      <div v-else-if="event.type === 'draw'" class="effect-token">
        <div v-for="n in Math.min(event.amount, 3)" :key="n" class="playing-card card-back" data-effect-card>
          <img :src="drawBack" alt="" />
        </div>
        <span v-if="event.amount > 1" class="effect-draw-count">+{{ event.amount }}</span>
      </div>
      <div v-else-if="'title' in event" class="effect-token effect-stamp">
        {{ event.title }}
      </div>
      <template v-else>
        <div class="effect-seat-flash"></div>
        <strong class="effect-token effect-hp">{{ event.type === 'heal' ? '+' : '−' }}{{ event.amount }}</strong>
      </template>
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
  inset: 0;
}
.effect-token, .effect-trace, .effect-seat-flash {
  position: absolute;
  top: 0;
  left: 0;
  opacity: 0;
}
.effect-token {
  display: flex;
  align-items: center;
  white-space: nowrap;
}
.effect-token > .playing-card + .playing-card {
  margin-left: calc(-0.43 * var(--card-width));
}
.effect-draw-count,
.effect-label {
  position: absolute;
  padding: 1px 5px;
  color: #fff0cd;
  background: #17221ff5;
  border: 1px solid #b89a66;
  font: 700 11px/1.45 var(--font-kai);
}
.effect-draw-count { right: -5px; top: -4px; }
.effect-label { left: 50%; bottom: -8px; transform: translateX(-50%); }
.effect-trace {
  height: 2px;
  transform-origin: 0 50%;
  background: linear-gradient(90deg, transparent, #c39855 30%, #f6deb1);
  box-shadow: 0 0 7px #dbad669e;
}
.effect-trace::after {
  position: absolute;
  right: 0;
  top: 50%;
  width: 5px;
  height: 5px;
  background: #ffedc5;
  box-shadow: inherit;
  transform: translate(50%, -50%) rotate(45deg);
  content: '';
}
.effect-surge {
  position: absolute;
  inset: 10% 6% 18%;
  opacity: 0;
  background: url('./assets/battle/cavalry.webp') center / contain no-repeat;
}
.effect-seat-flash {
  border: 1px solid #f19463;
  border-radius: 3px;
  background: radial-gradient(ellipse, #f5c5857a, #c9472a54 62%, #db674577);
  box-shadow: 0 0 19px #e4512e93, inset 0 0 14px #fff0acb3;
}
[data-effect='heal'] .effect-seat-flash {
  border-color: #ccedbd;
  background: radial-gradient(ellipse, #e4f4b86b, #53a98345 62%, #93d59d75);
  box-shadow: 0 0 17px #88b6a687, inset 0 0 13px #d8f6b382;
}
.effect-hp {
  color: #ffe0b3;
  font: 800 clamp(23px, 4.2dvh, 47px)/1 var(--font-kai);
  text-shadow: 0 1px 2px #452014, 0 0 11px #e4482b, 0 0 17px #992916;
}
[data-effect='heal'] .effect-hp {
  color: #e1ffca;
  text-shadow: 0 1px 2px #142622, 0 0 11px #2c9161, 0 0 17px #397863;
}
.effect-stamp {
  max-width: min(30vw, 250px);
  padding: 3px 10px;
  border: 1px solid #af915b;
  background: #17221fed;
  color: #f7e7bd;
  font: 700 clamp(13px, 2.4dvh, 24px)/1.25 var(--font-kai);
  text-align: center;
  white-space: normal;
}
[data-effect='death'] .effect-stamp { border-color: #f5b48e; background: #782d23ed; }
</style>
