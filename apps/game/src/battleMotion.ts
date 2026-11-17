import type { BattleEvent } from '@mak/rules'

// Events in one command overlap slightly, keeping responses in order without delaying long chains.
export function motionStep(events: readonly BattleEvent[]) {
  return Math.min(0.20, 0.85 / Math.max(1, events.length - 1))
}

export function settleDelay(events: readonly BattleEvent[]) {
  if (!events.length || localStorage.getItem('mak-reduced-motion') === 'true') return 300
  return Math.ceil(1000 * (0.64 + motionStep(events) * (events.length - 1)))
}
