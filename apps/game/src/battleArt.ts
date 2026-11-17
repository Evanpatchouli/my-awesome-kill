const portraits = import.meta.glob<string>('./assets/standard/character/*.jpg', { eager: true, query: '?url', import: 'default' })
const cards = import.meta.glob<string>('./assets/standard/card/*.png', { eager: true, query: '?url', import: 'default' })

export function heroPortrait(heroId: string): string {
  return portraits[`./assets/standard/character/${heroId}.jpg`] ?? ''
}

export function cardImage(cardKind: string): string {
  return cards[`./assets/standard/card/${cardKind}.png`] ?? ''
}
