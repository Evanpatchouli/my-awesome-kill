import type { Card } from './cards'
export type { Card } from './cards'
export type Game = import('./engine').Game
export type Seat = 0 | 1
export type BattleEvent =
  | { type: 'play' | 'respond' | 'equip' | 'judge' | 'discard'; seat: Seat; cards: Card[] }
  | { type: 'damage' | 'heal' | 'draw'; seat: Seat; amount: number }
import { heroes, type Hero } from './heroes'
export type { Hero } from './heroes'
export type Done = () => void
export interface Option {
  label: string
  run: Done
  score: number
  card?: number
}
export interface Player {
  hero: Hero
  name: string
  hp: number
  maxHp: number
  hand: Card[]
  equipment: Card[]
  judges: Card[]
}
export function player(hero: Hero): Player {
  const { hp, name } = heroes[hero]
  return { hero, hp, maxHp: hp, name, hand: [], equipment: [], judges: [] }
}
export function keepValue(c: Card): number {
  if (c.kind === 'tao') return 10
  if (c.kind === 'shan' || c.kind === 'wuxie') return 7
  if (c.kind === 'sha') return 4
  return 5
}
