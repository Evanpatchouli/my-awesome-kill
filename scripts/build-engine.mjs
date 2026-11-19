import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const root = fileURLToPath(new URL('../', import.meta.url))
const manifest = path.join(root, 'packages/duel-web/Cargo.toml')
const target = path.resolve(root, process.env.CARGO_TARGET_DIR || 'packages/duel-web/target')
const output = path.join(root, 'apps/game/src/generated/duel')

function run(tool, args) {
  const result = spawnSync(tool, args, { cwd: root, stdio: 'inherit' })
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}

run('cargo', [
  'build',
  '--locked',
  '--manifest-path',
  manifest,
  '--target-dir',
  target,
  '--target',
  'wasm32-unknown-unknown',
  '--release',
])
run('wasm-bindgen', [
  '--target',
  'web',
  '--out-dir',
  output,
  '--out-name',
  'duel',
  path.join(target, 'wasm32-unknown-unknown/release/duel_web.wasm'),
])
