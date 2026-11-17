import { Game } from './engine'
import type { Hero } from './model'

export function createMatch({ seed, hero }: { seed: number; hero: Hero }) {
  const game = new Game(seed, hero)
  return {
    view: () => game.view(),
    act: (revision: number, key: string) => game.actor === 0 && game.act(revision, key),
    discard: (revision: number, ids: number[]) => game.actor === 0 && game.discardCards(revision, ids),
    bot: () => game.actor === 1 && game.best(),
  }
}
export type { BattleEvent, Card, Hero, Seat } from './model'
export { heroes } from './heroes'

export { slot } from './cards'
export { definitions } from './cards'
