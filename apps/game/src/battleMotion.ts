import type { CardPrint } from './classicProtocol'

export type FeedbackEvent =
  | {
    type: 'play' | 'respond' | 'equip' | 'judge' | 'discard'
    seat: number
    target?: number
    event?: number
    cards: (CardPrint & { id?: number })[]
  }
  | { type: 'draw' | 'damage' | 'heal'; seat: number; amount: number }
  | { type: 'skill' | 'death'; seat: number; title: string }

export function motionStep(events: readonly FeedbackEvent[]) {
  return Math.min(0.2, 0.85 / Math.max(1, events.length - 1))
}

export function battleMotionReduced() {
  return localStorage.getItem('mak-reduced-motion') === 'true'
    || window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

export function settleDelay(events: readonly FeedbackEvent[]) {
  if (!events.length || battleMotionReduced()) return 300
  return Math.ceil(1000 * (0.64 + motionStep(events) * (events.length - 1)))
}
