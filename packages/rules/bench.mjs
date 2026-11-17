import { createRequire } from 'node:module'
import { cpus } from 'node:os'
import { fileURLToPath, pathToFileURL } from 'node:url'

// Vite is a dependency of Vitest, which is declared by this package.
const require = createRequire(import.meta.url)
const vitestRequire = createRequire(require.resolve('vitest/package.json'))
const { createServer } = await import(pathToFileURL(vitestRequire.resolve('vite')).href)
const root = fileURLToPath(new URL('.', import.meta.url))
const server = await createServer({
  root,
  configFile: false,
  server: { middlewareMode: true },
  appType: 'custom',
  logLevel: 'silent',
})
try {
  const { Game } = await server.ssrLoadModule('/src/engine.ts')
  const samples = []
  const play = (seed, collect) => {
    const game = new Game(seed, seed % 2 ? 'zhangfei' : 'huangyueying')
    game.view()
    let decisions = 0
    while (game.winner === null) {
      if (decisions >= 1500) throw new Error(`Seed ${seed} did not finish within 1500 decisions`)
      const start = process.hrtime.bigint()
      const acted = game.best()
      game.view()
      const elapsed = process.hrtime.bigint() - start
      if (!acted) throw new Error(`Seed ${seed} stalled at decision ${decisions}`)
      if (collect) samples.push(Number(elapsed) / 1e6)
      decisions++
    }
  }
  for (let seed = 1001; seed <= 1010; seed++) play(seed, false)
  for (let seed = 1; seed <= 100; seed++) play(seed, true)
  samples.sort((a, b) => a - b)
  const percentile = (p) => samples[Math.ceil(samples.length * p) - 1]
  console.log(
    JSON.stringify(
      {
        date: new Date().toISOString(),
        node: process.version,
        cpu: cpus()[0]?.model,
        games: 100,
        decisions: samples.length,
        unit: 'ms per decision (best + view)',
        p50: percentile(0.5),
        p95: percentile(0.95),
        p99: percentile(0.99),
        max: samples.at(-1),
      },
      null,
      2,
    ),
  )
} finally {
  await server.close()
}
