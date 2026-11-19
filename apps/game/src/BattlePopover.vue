<script setup lang="ts">
import { onBeforeUnmount, ref, useId, watch } from 'vue'

const props = withDefaults(defineProps<{
  title: string
  description: string
  delay?: number
}>(), { delay: 0 })

const anchor = ref<HTMLElement | null>(null)
const position = ref<{ x: number; top: number } | null>(null)
const tooltipId = useId()
let timer: number | undefined

function close() {
  window.clearTimeout(timer)
  timer = undefined
  position.value = null
  window.removeEventListener('scroll', close, true)
  window.removeEventListener('resize', close)
  window.removeEventListener('keydown', closeOnEscape)
}

function closeOnEscape(event: KeyboardEvent) {
  if (event.key === 'Escape') close()
}

function reveal() {
  timer = undefined
  if (!anchor.value) return
  const rect = anchor.value.getBoundingClientRect()
  position.value = { x: rect.left + rect.width / 2, top: rect.top }
}

function open(after: number) {
  close()
  if (!props.description?.trim()) return
  window.addEventListener('scroll', close, true)
  window.addEventListener('resize', close)
  window.addEventListener('keydown', closeOnEscape)
  if (after > 0) timer = window.setTimeout(reveal, after)
  else reveal()
}

function pointerEnter(event: PointerEvent) {
  if (event.pointerType !== 'touch') open(props.delay)
}

function focusIn(event: FocusEvent) {
  if (event.target instanceof HTMLElement && event.target.matches(':focus-visible')) open(0)
}

watch(() => props.description, close)
onBeforeUnmount(close)
</script>

<template>
  <span
    ref="anchor"
    class="battle-popover-anchor"
    @pointerenter="pointerEnter"
    @pointerleave="close"
    @pointerdown="close"
    @focusin="focusIn"
    @focusout="close"
  >
    <slot :describedBy="position ? tooltipId : undefined" />

    <Teleport to="body">
      <div
        v-if="position"
        :id="tooltipId"
        class="battle-popover"
        role="tooltip"
        :style="{ '--anchor-x': `${position.x}px`, '--anchor-top': `${position.top}px` }"
      >
        <strong v-if="title">{{ title }}</strong>
        <p>{{ description }}</p>
      </div>
    </Teleport>
  </span>
</template>

<style scoped>
.battle-popover-anchor {
  display: flex;
  flex: none;
  min-inline-size: 0;
  max-inline-size: 100%;
}
.battle-popover-anchor > :slotted(button:disabled) {
  pointer-events: none;
}
.battle-popover {
  --popover-width: min(20rem, calc(100vw - 16px));
  --side-clearance: calc(var(--popover-width) / 2 + 8px);
  position: fixed;
  z-index: 30;
  bottom: calc(100% - var(--anchor-top) + 8px);
  left: clamp(var(--side-clearance), var(--anchor-x), calc(100vw - var(--side-clearance)));
  transform: translateX(-50%);
  box-sizing: border-box;
  width: var(--popover-width);
  max-height: max(0px, calc(var(--anchor-top) - 16px));
  overflow: hidden;
  padding: 8px 11px;
  border: 1px solid #b08b62;
  border-radius: 5px;
  background: linear-gradient(115deg, #211a12, #17130e);
  color: #f6e8cd;
  box-shadow: 0 4px 13px #17130e88;
  font-size: clamp(12px, 2.2dvh, 16px);
  line-height: 1.4;
  overflow-wrap: anywhere;
  pointer-events: none;
}
.battle-popover strong {
  display: block;
  margin-bottom: 3px;
  color: #e5c18d;
}
.battle-popover p {
  margin: 0;
  white-space: pre-wrap;
}
@media (max-height: 350px) {
  .battle-popover {
    --popover-width: min(24rem, calc(100vw - 16px));
    font-size: 11px;
  }
}
</style>
