<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { gsap } from 'gsap'
import ClassicBattle from './ClassicBattle.vue'
import ClassicSelection from './ClassicSelection.vue'
import { useGame } from './game'

import { listMatches } from './persistence'
import { heroPortrait } from './battleArt'
import { describeEvent, generalById, type GeneralDefinition, type TableView } from './classicProtocol'
const zhangfei = heroPortrait('zhangfei')
const huangyueying = heroPortrait('huangyueying')

const route = useRoute()
const router = useRouter()
const game = useGame()
const hero = ref('zhangfei')
const count = ref(8)
const content = ref<string>()
const customGenerals = ref<GeneralDefinition[]>([])
const importStatus = ref('')
const importError = ref('')
const importPending = ref(false)
const replay = ref<Awaited<ReturnType<typeof game.inspectReplay>>>()
const replayView = ref<TableView>()
const replayLog = computed(() => {
  const snapshot = replayView.value
  return snapshot?.events.flatMap((event) => describeEvent(snapshot, event) ?? []) ?? []
})
const replayStep = ref(0)
const replayError = ref('')
const replayLoading = ref(false)
let replayVersion = 0
function closeReplay() {
  replayVersion++
  replay.value = undefined
  replayView.value = undefined
  replayError.value = ''
  replayLoading.value = false
}
async function openReplay(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  closeReplay()
  const version = replayVersion
  replayLoading.value = true
  try {
    if (file.size > 2 * 1024 * 1024) throw new Error('录像文件不能超过 2 MiB。')
    const source = await game.inspectReplay(await file.text())
    if (version !== replayVersion) return
    replayView.value = source.at(0)
    replay.value = source
    replayStep.value = 0
  } catch (error) {
    if (version === replayVersion) replayError.value = String(error)
  } finally {
    if (version === replayVersion) replayLoading.value = false
  }
}
function seekReplay(event: Event) {
  if (!replay.value) return
  const step = Number((event.target as HTMLInputElement).value)
  replayView.value = replay.value.at(step)
  replayStep.value = step
}
function downloadReplay() {
  const raw = game.exportContentReplay()
  if (!raw) return
  const url = URL.createObjectURL(new Blob([raw], { type: 'application/json' }))
  const link = document.createElement('a')
  link.href = url
  link.download = '随心杀-内容包录像.json'
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
let importVersion = 0
function clearImport() {
  importVersion++
  if (!generalById.has(hero.value)) hero.value = 'zhangfei'
  content.value = undefined
  customGenerals.value = []
  importStatus.value = ''
  importError.value = ''
  importPending.value = false
}
async function importContent(file: File) {
  clearImport()
  const version = importVersion
  if (file.size > 128 * 1024) {
    importError.value = '文件不能超过 128 KiB。'
    return
  }
  importStatus.value = '正在检查导入内容…'
  importPending.value = true
  try {
    const raw = await file.text()
    if (version !== importVersion) return
    const catalog = await game.inspectContent(raw)
    if (version !== importVersion) return
    content.value = raw
    customGenerals.value = catalog.generals
    importStatus.value = `已导入：${file.name} · ${catalog.cards.length} 牌种 / ${catalog.generals.length} 将`
  } catch (error) {
    if (version !== importVersion) return
    importStatus.value = ''
    importError.value = error instanceof Error ? error.message : '导入失败，请检查 JSON 文件。'
  } finally {
    if (version === importVersion) importPending.value = false
  }
}
const reduced = ref(localStorage.getItem('mak-reduced-motion') === 'true')
const records = ref<Awaited<ReturnType<typeof listMatches>>>([])
const screen = ref<HTMLElement | null>(null)
const backTarget = computed(() =>
  route.path === '/battle' ? '/classic' : route.path === '/classic' ? '/modes' : '/',
)
function back() {
  if (route.path === '/battle') game.leave()
  void router.push(backTarget.value)
}
async function retryHistory() {
  await game.retrySave()
  try {
    records.value = await listMatches()
  } catch {
    records.value = []
  }
}
async function begin() {
  if (await game.start(hero.value, count.value, content.value)) void router.push('/battle')
}
watch(
  reduced,
  (value) => {
    localStorage.setItem('mak-reduced-motion', String(value))
    document.documentElement.classList.toggle('reduced-motion', value)
  },
  { immediate: true },
)
watch(
  () => route.path,
  async (path, old) => {
    if (path !== '/classic') closeReplay()
    if (
      (old === '/battle' && path !== '/battle') ||
      (old === '/classic' && game.busy && path !== '/battle')
    )
      game.leave()
    if (path === '/battle' && !game.view) {
      void router.replace('/classic')
      return
    }
    if (path === '/profile') {
      try {
        records.value = await listMatches()
      } catch {
        records.value = []
      }
    }
    await nextTick()
    if (path === '/battle') window.scrollTo(0, 0)
    if (screen.value) gsap.killTweensOf(screen.value)
    if (!reduced.value && screen.value)
      gsap.fromTo(
        screen.value,
        { opacity: 0, y: 10 },
        { opacity: 1, y: 0, duration: 0.35, ease: 'power2.out' },
      )
  },
)
onMounted(() => {
  if (route.path === '/battle' && !game.view) void router.replace('/classic')
})
onUnmounted(() => {
  importVersion++
  game.leave()
  if (screen.value) gsap.killTweensOf(screen.value)
})
</script>

<template>
  <main ref="screen" class="app-shell">
    <template v-if="route.path === '/'">
      <div class="home-page">
        <nav class="home-nav" aria-label="首页导航">
          <button class="nav-item" @click="router.push('/coming-soon')">
            <span class="nav-circle nav-coming" aria-hidden="true"></span><span>尽请期待</span>
          </button>
          <div class="nav-right">
            <button class="nav-item" @click="router.push('/profile')">
              <span class="nav-circle nav-profile" aria-hidden="true"></span
              ><span>我的</span></button
            ><button class="nav-item" @click="router.push('/settings')">
              <span class="nav-circle nav-settings" aria-hidden="true"></span><span>设置</span>
            </button>
          </div>
        </nav>
        <div class="home-art" aria-hidden="true"></div>

        <div
          class="home-title"
          v-motion
          :initial="reduced ? { opacity: 1 } : { opacity: 0, y: 16 }"
          :enter="{ opacity: 1, y: 0, transition: { duration: 700 } }"
        >
          <h1><span class="sr-only">随心杀</span></h1>
        </div>
        <button class="start-seal" @click="router.push('/modes')">
          <span class="sr-only">开始游玩</span>
        </button>
      </div>
    </template>
    <template v-else>
      <header class="page-header">
        <button class="back-button" aria-label="返回" @click="back"></button>
      </header>
      <section v-if="route.path === '/modes'" class="page-content modes-page">
        <div class="section-heading">
          <div>
            <h1>选择玩法</h1>
          </div>
        </div>
        <button class="mode-feature" @click="router.push('/classic')">
          <div class="mode-art">
            <img :src="zhangfei" alt="" /><img :src="huangyueying" alt="" />
          </div>
          <div class="mode-copy">
            <h2>经典三国杀</h2>

            <span class="mode-link">进入玩法 <b>↗</b></span>
          </div>
        </button>
      </section>
      <section v-else-if="route.path === '/classic' && replayView && replay" class="content-replay">
        <div class="replay-bar">
          <button type="button" @click="closeReplay">退出回放</button>
          <label>录像进度 {{ replayStep }} / {{ replay.total }}
            <input type="range" :value="replayStep" min="0" :max="replay.total" @change="seekReplay" />
          </label>
        </div>
        <ClassicBattle :view="replayView" :locked="true" :log="replayLog" :feedback="{ revision: 0, events: [] }"
          :save-error="false" error="" readonly />
      </section>
      <ClassicSelection
        v-else-if="route.path === '/classic'"
        v-model:hero="hero"
        v-model:count="count"
        :busy="game.busy || importPending"
        :error="game.error"
        :custom-generals="customGenerals"
        :import-status="importStatus"
        :import-error="importError"
        @import="importContent"
        @clear-import="clearImport"
        @start="begin"
      >
        <label>打开内容包录像 <input type="file" accept=".json,application/json" :disabled="replayLoading" @change="openReplay" /></label>
        <span v-if="replayError" role="alert">{{ replayError }}</span>
      </ClassicSelection>
      <ClassicBattle
        v-else-if="route.path === '/battle' && game.view"
        :view="game.view"
        :locked="game.locked"
        :error="game.error"
        :log="game.log"
        :feedback="game.feedback"
        :save-error="game.saveError"
        @act="game.act"
        @restart="begin"
        @retry-save="game.retrySave"
      >
        <button v-if="content && (game.view.winner || game.view.drawn)" type="button" class="primary-button" @click="downloadReplay">
          导出内容包录像
        </button>
      </ClassicBattle>
      <section v-else-if="route.path === '/profile'" class="page-content simple-page">
        <div class="section-heading">
          <div>
            <h1>我的</h1>
            <p>每一局，都值得记下。</p>
          </div>
        </div>
        <h2>对局记录</h2>
        <p v-if="game.saveError || game.pendingRecords.length" class="save-error">
          {{ game.pendingRecords.length }} 条战绩待保存。<button @click="retryHistory">
            重试保存
          </button>
        </p>
        <div v-if="records.length" class="record-list">
          <div v-for="record in records" :key="record.id">
            <span>{{ record.summary }}</span
            ><span
              >胜者 {{ record.winner }} · {{ new Date(record.finishedAt).toLocaleString() }}</span
            >
          </div>
        </div>
        <div v-else class="empty-state">
          <span>未</span>
          <p>还没有对局记录<br /><small>去开启你的第一场对局吧。</small></p>
        </div>
      </section>
      <section v-else-if="route.path === '/settings'" class="page-content simple-page">
        <div class="section-heading">
          <div>
            <h1>设置</h1>
            <p>让每一次落牌都更自在。</p>
          </div>
        </div>
        <div class="setting-row">
          <div>
            <h2>减少动态效果</h2>
            <p>关闭页面过渡与装饰动画。</p>
          </div>
          <button
            role="switch"
            :aria-checked="reduced"
            class="toggle"
            :class="{ on: reduced }"
            @click="reduced = !reduced"
          >
            <span></span>
          </button>
        </div>
      </section>
      <section v-else class="page-content simple-page">
        <div class="section-heading">
          <div>
            <h1>尽请期待</h1>
            <p>新的故事，正在酝酿。</p>
          </div>
        </div>
        <div class="empty-state">
          <span>待</span>
          <p>山高水长，后会有期。</p>
        </div>
      </section>
    </template>
    <p v-if="game.error && route.path === '/battle' && !game.view?.choice?.moveCards" role="alert" class="game-error">
      {{ game.error }}
    </p>
  </main>
</template>
