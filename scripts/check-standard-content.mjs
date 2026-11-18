// Optional upstream conformance check. No reference files are loaded by the game.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'

const project = new URL('../', import.meta.url)
const reference = new URL('.reference/noname/apps/core/', project)
const data = new URL('packages/duel-core/data/', project)
const assets = new URL('apps/game/src/assets/standard/', project)
const read = (base, relative) => readFile(new URL(relative, base), 'utf8')
const digest = async (base, relative) => createHash('sha256').update(await readFile(new URL(relative, base))).digest('hex')

const generals = JSON.parse(await read(data, 'standard-2008-generals.json'))
const sort = await read(reference, 'character/standard/sort.js')
const expectedIds = JSON.parse(sort.match(/standard_2008:\s*(\[[^\]]+\])/)[1])
assert.deepEqual(generals.map(g => g.id), expectedIds)
const characters = await read(reference, 'character/standard/character.js')
const translations = new Map()
const strings = await read(reference, 'character/standard/translate.js')
for (const [, key, literal] of strings.matchAll(/^\s*(\w+):\s*("(?:[^"\\]|\\.)*"),/gm)) {
  translations.set(key, JSON.parse(literal))
}
for (const g of generals) {
  const block = characters.match(new RegExp('\\s'+g.id+': \\{([^}]+)\\}'))[1]
  assert.equal(g.name, translations.get(g.id))
  assert.equal(g.gender, block.match(/sex: "(\w+)"/)[1])
  assert.equal(g.faction, block.match(/group: "(\w+)"/)[1])
  assert.equal(g.maxHp, Number(block.match(/hp: (\d+)/)[1]))
  assert.equal(g.isLord, block.includes('isZhugong: true'))
  assert.deepEqual(g.skills.map(s => s.id), JSON.parse(block.match(/skills: (\[.*?\])/)[1]))
  for (const skill of g.skills) {
    assert.equal(skill.name, translations.get(skill.id))
    assert.equal(skill.description, translations.get(skill.id + '_info'))
  }
  const file = 'character/' + g.id + '.jpg'
  assert.equal(await digest(assets, file), await digest(reference, 'image/' + file), file)
}
const deck = JSON.parse(await read(data, 'standard-deck.json'))
const list = (await read(reference, 'card/standard.js')).split(/\n\s*list:\s*\[/).at(-1)
const tuples = Array.from(list.matchAll(/\["(spade|club|heart|diamond)",\s*(\d+),\s*"(\w+)"\]/g), ([,suit,rank,kind],id) => ({id,kind,suit,rank:Number(rank)}))
assert.equal(tuples.length, 108)
assert.deepEqual(deck, tuples.slice(0,104))
const kinds = new Set(deck.map(c => c.kind))
for (const kind of kinds) {
  const file = 'card/' + kind + '.png'
  assert.equal(await digest(assets, file), await digest(reference, 'image/' + file), file)
}
console.log(`Verified ${generals.length} generals, ${deck.length} physical cards and ${kinds.size + generals.length} original images.`)

const extraAssets = new URL('apps/game/src/assets/extra/', project)
const tiesuo = 'card/tiesuo.png'
assert.equal(await digest(extraAssets, tiesuo), await digest(reference, 'image/' + tiesuo), tiesuo)
console.log('Verified the extra-pack Tiesuo illustration against the reference.')
