<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { gsap } from 'gsap'
import BattleView from './BattleView.vue'
import { useGame } from './game'
import { heroes } from '@mak/rules'
import { listMatches } from './persistence'
import zhangfei from './assets/ink/zhangfei.webp'
import huangyueying from './assets/ink/huangyueying.webp'

const route = useRoute()
const router = useRouter()
const game = useGame()
const hero = ref<'zhangfei' | 'huangyueying'>('zhangfei')
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
function begin() {
  if (game.start(hero.value)) void router.push('/battle')
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
    if (old === '/battle' && path !== '/battle') game.leave()
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
        <div v-if="route.path === '/battle' && game.view" class="header-info">
          第 {{ game.view.turn }} 回合　·　牌堆 {{ game.view.drawCount }}　·　弃牌
          {{ game.view.discardCount }}
        </div>
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
      <section v-else-if="route.path === '/classic'" class="page-content selection-page">
        <div class="section-heading">
          <div>
            <h1>选择你的武将</h1>
            <p>单人对战电脑，另一位武将将成为你的对手。</p>
          </div>
        </div>
        <div class="hero-grid grid grid-cols-1 md:grid-cols-2 gap-6">
          <button
            v-for="item in [
              { id: 'zhangfei', ...heroes.zhangfei, image: zhangfei },
              { id: 'huangyueying', ...heroes.huangyueying, image: huangyueying },
            ] as const"
            :key="item.id"
            class="hero-option"
            :class="{ active: hero === item.id }"
            @click="hero = item.id"
          >
            <img :src="item.image" :alt="item.name + '立绘'" />
            <div class="hero-info">
              <h2>{{ item.name }}</h2>
              <p>{{ item.skills.join(' ') }}</p>
            </div>
            <span class="hero-check">{{ hero === item.id ? '✓' : '＋' }}</span>
          </button>
        </div>
        <div class="selection-bottom flex items-center justify-between">
          <p>
            你执 <strong>{{ hero === 'zhangfei' ? '张飞' : '黄月英' }}</strong
            >，电脑执 {{ hero === 'zhangfei' ? '黄月英' : '张飞' }}
          </p>
          <button class="primary-button" @click="begin">开始对局 <span>→</span></button>
        </div>
      </section>
      <BattleView v-else-if="route.path === '/battle'" @exit="back" @restart="begin" />
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
  </main>
</template>
