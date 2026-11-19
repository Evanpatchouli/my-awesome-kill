import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fs.realpathSync(fileURLToPath(new URL('../', import.meta.url)))
const target = path.join(root, 'packages/duel-web/target')
const generated = path.join(root, 'apps/game/src/generated/duel')
const features = ['compare', 'elemental', 'recast', 'qiaoshui', 'zhongyi', 'phase-skills', 'multi-compare', 'mingdian', 'xunxun', 'vitality', 'revival', 'rest', 'judge-area', 'equip-slots']
const help = `Usage: node scripts/test-feature-tables.mjs --output <new-external-directory> [--url <url>] [--feature <name>]...\n\nRuns serial QA builds and browser smoke tests against an already running game server.\nFeatures: ${features.join(', ')}\nMAK_QA_BROWSER and MAK_QA_PLAYWRIGHT are inherited. When MAK_SMOKE_SCREENSHOTS\nis set, each feature writes to a new subdirectory under that external directory.\n`

function parseArgs(args) {
  const options = { selected: [] }
  for (let i = 0; i < args.length; i++) {
    const arg = args[i]
    if (arg === '--help') { console.log(help); process.exit(0) }
    if (!['--output', '--url', '--feature'].includes(arg)) throw new Error(`Unknown option: ${arg}`)
    const value = args[++i]
    if (!value || value.startsWith('--')) throw new Error(`Missing value for ${arg}`)
    if (arg === '--feature') {
      if (!features.includes(value)) throw new Error(`Unknown feature: ${value}`)
      if (options.selected.includes(value)) throw new Error(`Duplicate feature: ${value}`)
      options.selected.push(value)
    } else {
      const key = arg.slice(2)
      if (options[key] !== undefined) throw new Error(`Duplicate option: ${arg}`)
      options[key] = value
    }
  }
  if (!options.output) throw new Error('--output is required')
  if (options.url) {
    const url = new URL(options.url)
    if (!['http:', 'https:'].includes(url.protocol)) throw new Error('--url must be HTTP(S)')
  }
  if (!options.selected.length) options.selected = features
  return options
}

// Resolve existing ancestors so a symlink cannot disguise a path inside the repository.
function resolvedPath(input) {
  let current = path.resolve(input)
  const suffix = []
  while (!fs.existsSync(current)) {
    const parent = path.dirname(current)
    if (parent === current) throw new Error(`Cannot resolve path: ${input}`)
    suffix.unshift(path.basename(current))
    current = parent
  }
  return path.join(fs.realpathSync(current), ...suffix)
}
function inside(parent, child) {
  const relative = path.relative(parent, child)
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative))
}
function external(input, label) {
  const result = resolvedPath(input)
  if (inside(root, result)) throw new Error(`${label} must be outside the repository`)
  return result
}
function digest(file) {
  const data = fs.readFileSync(file)
  return { bytes: data.length, sha256: createHash('sha256').update(data).digest('hex') }
}
function snapshot(dir) {
  if (!fs.existsSync(dir)) return null
  const entries = {}
  function visit(current, prefix = '') {
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const relative = path.join(prefix, entry.name)
      const full = path.join(current, entry.name)
      if (entry.isDirectory()) visit(full, relative)
      else if (entry.isFile()) entries[relative] = digest(full)
      else entries[relative] = { type: 'other', value: fs.readlinkSync(full) }
    }
  }
  visit(dir)
  return entries
}
function run(command, args, log, env = process.env) {
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 })
  fs.writeFileSync(log, `$ ${command} ${args.join(' ')}\n${result.stdout || ''}${result.stderr || ''}${result.error ? `\n${result.error.message}\n` : ''}`)
  if (result.error || result.status !== 0) throw new Error(`${command} failed (${result.error?.message || `exit ${result.status ?? result.signal}`}); see ${log}`)
}

function main() {
  const options = parseArgs(process.argv.slice(2))
  const output = external(options.output, '--output')
  if (fs.existsSync(output)) throw new Error(`Output already exists: ${output}`)
  const screenshotRoot = process.env.MAK_SMOKE_SCREENSHOTS && external(process.env.MAK_SMOKE_SCREENSHOTS, 'MAK_SMOKE_SCREENSHOTS')
  if (screenshotRoot && (inside(output, screenshotRoot) || inside(screenshotRoot, output))) throw new Error('Screenshot and output directories must not overlap')
  if (screenshotRoot) for (const feature of options.selected) {
    if (fs.existsSync(path.join(screenshotRoot, feature))) throw new Error(`Screenshot directory already exists: ${path.join(screenshotRoot, feature)}`)
  }
  const before = snapshot(generated)
  fs.mkdirSync(output, { recursive: false })
  const summary = { started: new Date().toISOString(), output, url: options.url || process.env.MAK_SMOKE_URL || 'http://127.0.0.1:1420', features: {}, generatedUnchanged: null }
  const lock = path.join(target, '.feature-tables-lock')
  let locked = false
  try {
    fs.mkdirSync(target, { recursive: true })
    try { fs.mkdirSync(lock); locked = true } catch (error) {
      if (error.code === 'EEXIST') throw new Error(`Another feature-table run owns ${lock}; remove it only after confirming that run has stopped`)
      throw error
    }
    for (const feature of options.selected) {
      const dir = path.join(output, feature)
      fs.mkdirSync(dir)
      const result = summary.features[feature] = { status: 'failed' }
      console.log(`[${feature}] building and testing`)
      try {
        run('cargo', ['build', '--locked', '--manifest-path', path.join(root, 'packages/duel-web/Cargo.toml'), '--target-dir', target, '--target', 'wasm32-unknown-unknown', '--release', '--features', `qa-${feature}`], path.join(dir, 'build.log'))
        const bindings = path.join(dir, 'bindings')
        run('wasm-bindgen', ['--target', 'web', '--out-dir', bindings, '--out-name', 'duel', path.join(target, 'wasm32-unknown-unknown/release/duel_web.wasm')], path.join(dir, 'bindgen.log'))
        result.artifacts = Object.fromEntries(fs.readdirSync(bindings).sort().filter(name => fs.statSync(path.join(bindings, name)).isFile()).map(name => [name, digest(path.join(bindings, name))]))
        const variable = feature === 'multi-compare' ? 'MAK_MULTI_BINDINGS' : feature === 'phase-skills' ? 'MAK_PHASE_BINDINGS' : `MAK_${feature.toUpperCase().replaceAll('-', '_')}_BINDINGS`
        const env = { ...process.env, [variable]: bindings }
        if (options.url) env.MAK_SMOKE_URL = options.url
        if (screenshotRoot) env.MAK_SMOKE_SCREENSHOTS = path.join(screenshotRoot, feature)
        run(process.execPath, [path.join(root, `apps/game/smoke-${feature}.mjs`)], path.join(dir, 'smoke.log'), env)
        result.status = 'passed'
        console.log(`[${feature}] passed`)
      } catch (error) {
        result.error = error.message
        console.error(`[${feature}] ${error.message}`)
      }
    }
  } finally {
    if (locked) fs.rmdirSync(lock)
    summary.generatedUnchanged = JSON.stringify(before) === JSON.stringify(snapshot(generated))
    summary.finished = new Date().toISOString()
    fs.writeFileSync(path.join(output, 'summary.json'), `${JSON.stringify(summary, null, 2)}\n`)
  }
  if (!summary.generatedUnchanged) throw new Error('Ordinary generated bindings changed during the run')
  if (Object.values(summary.features).some(result => result.status !== 'passed')) process.exitCode = 1
}
try { main() } catch (error) { console.error(error.message); process.exitCode = 1 }
