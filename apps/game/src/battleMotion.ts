import type { Card } from '@mak/rules'

export type FeedbackEvent =
  | {
    type: 'play' | 'respond' | 'equip' | 'judge' | 'discard'
    seat: number
    cards: Pick<Card, 'id' | 'kind' | 'name'>[]
  }
  | { type: 'draw' | 'damage' | 'heal'; seat: number; amount: number }

export function motionStep(events: readonly FeedbackEvent[]) {
  return Math.min(0.2, 0.85 / Math.max(1, events.length - 1))
}
export function settleDelay(events: readonly FeedbackEvent[]) {
  if (!events.length || localStorage.getItem('mak-reduced-motion') === 'true') return 300
  return Math.ceil(1000 * (0.64 + motionStep(events) * (events.length - 1)))
}
