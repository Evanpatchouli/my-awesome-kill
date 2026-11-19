import assert from 'node:assert/strict'
import { createRequire } from 'node:module'

// Run on a fresh packaged WebView2 QA instance with a private data directory.
const { chromium } = createRequire(import.meta.url)(process.env.MAK_QA_PLAYWRIGHT || 'playwright')
const browser = await chromium.connectOverCDP(process.env.MAK_CDP_URL)
try {
  const page = browser.contexts().flatMap(context => context.pages())
    .find(tab => new URL(tab.url()).hostname === 'tauri.localhost')
  assert.ok(page, 'Expected the packaged game')
  const errors = []
  page.on('pageerror', error => errors.push(error.message))
  await page.addInitScript(() => {
    Math.random = () => 16.1 / 0x7fffffff
    localStorage.setItem('mak-reduced-motion', 'true')
  })
  await page.goto(new URL('/?equip-smoke', page.url()).href)
  const button = name => page.getByRole('button', { name, exact: true })
  await page.getByRole('button', { name: /开始\s*游玩/ }).click()
  await page.getByRole('button', { name: /经典三国杀/ }).click()
  await button('关羽').click()
  await page.locator('select').selectOption('8')
  await button('开始对局').click()
  const board = page.locator('[data-battle-table]')
  const zone = page.locator('.classic-self .classic-zones')
  const confirm = button('确认')
  let previous
  for (const [id, name] of [[69, '麒麟弓'], [65, '青龙偃月刀']]) {
    const card = page.locator(`[data-hand] [data-card-id="${id}"]`)
    await card.waitFor()
    assert.match(await card.getAttribute('aria-label'), new RegExp(name))
    const version = Number(await board.getAttribute('data-revision'))
    await card.click()
    assert.ok(await confirm.isEnabled(), `Selecting ${name} should allow immediate confirmation`)
    await card.click()
    assert.ok(await confirm.isDisabled(), 'Deselecting clears the proposed play')
    await card.click()
    await confirm.click()
    await card.waitFor({ state: 'detached' })
    assert.match(await zone.innerText(), new RegExp(name))
    if (previous) assert.ok(!(await zone.innerText()).includes(previous))
    assert.equal(Number(await board.getAttribute('data-revision')), version + 1)
    previous = name
    console.log(`equipped ${name}: ${version} -> ${version + 1}`)
  }
  assert.deepEqual(errors, [])
} finally {
  await browser.close()
}
