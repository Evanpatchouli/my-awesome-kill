import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { copyFile, mkdir, mkdtemp, open, readFile, readdir, rename, rmdir, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const app = path.join(root, 'apps/game')
const native = path.join(app, 'src-tauri')
const android = path.join(native, 'gen/android')
const target = 'x86_64-pc-windows-msvc'
const executable = 'my-awesome-kill.exe'
const require = createRequire(path.join(app, 'package.json'))

function run(command, args, { cwd = root, env = process.env, capture = false } = {}) {
  const result = spawnSync(command, args, {
    cwd,
    env,
    encoding: 'utf8',
    stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit',
  })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(`${path.basename(command)} 失败（${result.signal ?? result.status}）。`)
  return result.stdout
}

function releaseVersion(version) {
  if (typeof version !== 'string' || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version))
    throw new Error('根 package.json 的 version 必须是正式版本号，例如 0.0.1。')
  const [major, minor, patch] = version.split('.').map(Number)
  const versionCode = major * 1_000_000 + minor * 1000 + patch
  if (minor >= 1000 || patch >= 1000 || versionCode < 1 || versionCode > 2_100_000_000)
    throw new Error('版本号超出 Android versionCode 范围。')
  return { version, versionCode }
}

async function androidTools() {
  const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT
  if (!sdk) throw new Error('请设置 ANDROID_HOME 指向 Android SDK。')
  const tools = path.resolve(sdk, 'build-tools')
  const installed = await readdir(tools, { withFileTypes: true })
  const version = installed.filter((entry) => entry.isDirectory() && /^\d+\.\d+\.\d+$/.test(entry.name))
    .map(({ name }) => name)
    .sort((left, right) => right.localeCompare(left, 'en', { numeric: true }))
    .find((name) => ['aapt.exe', 'lib/apksigner.jar'].every((file) => existsSync(path.join(tools, name, file))))
  if (!version) throw new Error('Android SDK 缺少带 aapt 和 apksigner 的 Build Tools。')
  return {
    aapt: path.join(tools, version, 'aapt.exe'),
    signer: path.join(tools, version, 'lib/apksigner.jar'),
    java: process.env.JAVA_HOME ? path.join(process.env.JAVA_HOME, 'bin/java.exe') : 'java.exe',
  }
}

async function collectApk(version, versionCode, stagedApk, tools, local) {
  const directory = path.join(android, 'app/build/outputs/apk/arm64/release')
  const metadata = JSON.parse(await readFile(path.join(directory, 'output-metadata.json'), 'utf8'))
  const entry = metadata.elements?.length === 1 ? metadata.elements[0] : null
  if (metadata.applicationId !== 'com.mak.game' || entry?.versionName !== version ||
      entry?.versionCode !== versionCode || !/^[\w.-]+\.apk$/.test(entry.outputFile ?? ''))
    throw new Error('Gradle 产物不符合预期：需要唯一且版本一致的 ARM64 APK。')
  await copyFile(path.join(directory, entry.outputFile), stagedApk)
  const badging = run(tools.aapt, ['dump', 'badging', stagedApk], { capture: true })
  const pkg = /^package: name='com\.mak\.game' versionCode='(\d+)' versionName='([^']+)'/m.exec(badging)
  if (Number(pkg?.[1]) !== versionCode || pkg?.[2] !== version ||
      !/^native-code:\s+'arm64-v8a'\s*$/m.test(badging) || /^application-debuggable/m.test(badging))
    throw new Error('APK 的包名、版本、架构或发布模式验证失败。')
  const signature = run(tools.java, ['-jar', tools.signer, 'verify', '--print-certs', stagedApk], { capture: true })
  if (!local && /^Signer #\d+ certificate DN: .*\bCN=Android Debug(?:,|$)/mi.test(signature))
    throw new Error('正式发布不能使用 Android 调试密钥。')
}

async function main() {
  const args = process.argv.slice(2)
  if (args.length > 1 || (args.length && args[0] !== '--local'))
    throw new Error('用法：pnpm build:release [--local]')
  if (process.platform !== 'win32')
    throw new Error('此命令需要 Windows 主机，以便同时生成 Windows EXE 和 Android APK。')
  const local = args[0] === '--local'
  const { version, versionCode } = releaseVersion(JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8')).version)
  const output = path.join(root, 'release')
  const destination = path.join(output, version)
  if (existsSync(destination)) throw new Error(`版本目录已存在，不会覆盖：${destination}`)
  if (!local && !existsSync(path.join(android, 'keystore.properties')))
    throw new Error('缺少 Android 发布签名 keystore.properties；仅本机试玩可加 --local。')
  const tools = await androidTools()
  const cli = path.join(path.dirname(require.resolve('@tauri-apps/cli/package.json')), 'tauri.js')
  const env = { ...process.env, MAK_ANDROID_LOCAL_SIGNING: local ? '1' : '0' }
  if (env.CARGO_TARGET_DIR) env.CARGO_TARGET_DIR = path.resolve(root, env.CARGO_TARGET_DIR)
  const cargo = JSON.parse(run('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1'], {
    cwd: native, env, capture: true,
  }))
  const desktop = path.join(cargo.target_directory, target, 'release', executable)
  const lock = path.join(output, '.build.lock')
  await mkdir(output, { recursive: true })
  const handle = await open(lock, 'wx').catch((error) => {
    if (error.code === 'EEXIST') throw new Error(`另一个发布构建正在使用当前工作区：${lock}`)
    throw error
  })
  let temporary
  let staging
  const exeName = `my-awesome-kill-${version}-windows-x64.exe`
  const apkName = `my-awesome-kill-${version}-android-arm64${local ? '-local' : ''}.apk`
  try {
    if (existsSync(destination)) throw new Error(`版本目录已存在，不会覆盖：${destination}`)
    temporary = await mkdtemp(path.join(tmpdir(), 'mak-release-'))
    const config = path.join(temporary, 'release.json')
    await writeFile(config, JSON.stringify({
      version,
      build: { beforeBuildCommand: null },
      bundle: { android: { versionCode, autoIncrementVersionCode: false } },
    }))
    const pnpm = process.env.npm_execpath
    if (!pnpm) throw new Error('请从仓库根目录运行 pnpm build:release。')
    console.log(`构建 ${version}：前端和 WASM`)
    if (pnpm.toLowerCase().endsWith('.exe')) run(pnpm, ['run', 'build'], { env })
    else run(process.execPath, [pnpm, 'run', 'build'], { env })
    console.log(`构建 ${version}：Windows x64 release EXE`)
    run(process.execPath, [cli, 'build', '--ci', '--no-bundle', '--target', target, '--config', config], { cwd: app, env })
    staging = await mkdtemp(path.join(output, '.staging-'))
    const stagedExe = path.join(staging, exeName)
    await copyFile(desktop, stagedExe)
    const actualVersion = run('powershell.exe', [
      '-NoProfile', '-NonInteractive', '-Command',
      '(Get-Item -LiteralPath $env:MAK_RELEASE_EXE).VersionInfo.ProductVersion',
    ], { env: { ...env, MAK_RELEASE_EXE: stagedExe }, capture: true }).trim()
    if (actualVersion !== version) throw new Error(`EXE 版本校验失败：预期 ${version}，实际 ${actualVersion}。`)
    console.log(`构建 ${version}：Android ARM64 release APK${local ? '（仅本地测试签名）' : ''}`)
    run(process.execPath, [cli, 'android', 'build', '--ci', '--apk', '--target', 'aarch64', '--split-per-abi', '--config', config], { cwd: app, env })
    await collectApk(version, versionCode, path.join(staging, apkName), tools, local)
    await rename(staging, destination)
    staging = undefined
    console.log(`发布完成：\n${path.join(destination, exeName)}\n${path.join(destination, apkName)}`)
  } finally {
    try {
      if (staging) {
        for (const name of [exeName, apkName]) {
          await unlink(path.join(staging, name)).catch((error) => { if (error.code !== 'ENOENT') throw error })
        }
        await rmdir(staging)
      }
      if (temporary) {
        await unlink(path.join(temporary, 'release.json')).catch((error) => { if (error.code !== 'ENOENT') throw error })
        await rmdir(temporary)
      }
    } finally {
      await handle.close()
      await unlink(lock)
    }
  }
}

main().catch((error) => {
  console.error(error.message)
  process.exitCode = 1
})
