<script setup lang="ts">
import { computed, ref } from 'vue'
import { factions, generals, type GeneralDefinition } from './classicProtocol'
import { heroPortrait } from './battleArt'

const hero = defineModel<string>('hero', { required: true })
const count = defineModel<number>('count', { required: true })
const props = defineProps<{
  busy: boolean
  error: string
  customGenerals: GeneralDefinition[]
  importStatus: string
  importError: string
}>()
const emit = defineEmits<{ start: []; import: [file: File]; clearImport: [] }>()
const faction = ref('')
const catalog = computed(() => {
  const byId = new Map<string, GeneralDefinition>(generals.map((item) => [item.id, item]))
  for (const item of props.customGenerals) byId.set(item.id, item)
  return [...byId.values()]
})
const roster = computed(() => catalog.value.filter((g) => !faction.value || g.faction === faction.value))
const selected = computed(() => catalog.value.find((item) => item.id === hero.value) ?? generals[0]!)
function chooseFile(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (file) emit('import', file)
  input.value = ''
}
</script>
<template>
  <section class="pick-page">
    <header class="pick-heading">
      <div>
        <h1>选择你的武将</h1>
        <p>标准版 · {{ generals.length }} 将<span v-if="customGenerals.length"> · 自定义 {{ customGenerals.length }} 将</span></p>
      </div>
      <label
        >身份局人数
        <select v-model.number="count">
          <option v-for="n in 9" :key="n" :value="n + 1">{{ n + 1 }} 人</option>
        </select></label
      >
    </header>
    <div class="pick-import">
      <label>导入本地 JSON <input type="file" accept=".json,application/json" @change="chooseFile" /></label>
      <button v-if="importStatus || importError" type="button" @click="emit('clearImport')">清除</button>
      <span v-if="importStatus" role="status">{{ importStatus }}</span>
      <span v-if="importError" role="alert">{{ importError }}</span>
      <slot />
    </div>
    <div class="pick-layout">
      <div class="pick-roster">
        <nav aria-label="筛选势力">
          <button
            v-for="(label, id) in { '': '全部', ...factions }"
            :key="id"
            :aria-pressed="faction === id"
            @click="faction = String(id)"
          >
            {{ label }}
          </button>
        </nav>
        <div class="pick-grid" aria-label="可选武将">
          <button
            v-for="item in roster"
            :key="item.id"
            :aria-label="item.name"
            :aria-pressed="hero === item.id"
            @click="hero = item.id"
          >
            <img v-if="heroPortrait(item.id)" :src="heroPortrait(item.id)" alt="" loading="lazy" /><span>{{ item.name }}</span>
          </button>
        </div>
      </div>
      <aside class="pick-detail">
        <img v-if="heroPortrait(selected.id)" :src="heroPortrait(selected.id)" :alt="selected.name + '立绘'" />
        <div class="pick-description">
          <p>{{ factions[selected.faction] ?? selected.faction }} · {{ selected.maxHp }} 体力</p>
          <h2>{{ selected.name }}</h2>
          <p v-for="skill in selected.skills" :key="skill.id">
            <b>{{ skill.name }}</b
            >　{{ skill.description }}
          </p>
        </div>
        <p v-if="error" role="alert">{{ error }}</p>
        <button class="primary-button" :disabled="busy" @click="emit('start')">
          {{ busy ? '正在开局…' : '开始对局' }}
        </button>
      </aside>
    </div>
  </section>
</template>
<style scoped>
.pick-page {
  max-width: 1420px;
  margin: auto;
  padding: clamp(12px, 3vw, 42px);
  color: #31291f;
}
.pick-heading,
.pick-heading label,
.pick-roster nav {
  display: flex;
  align-items: center;
  gap: 12px;
}
.pick-heading {
  justify-content: space-between;
  padding-bottom: 16px;
  border-bottom: 1px solid #c9b59b;
}
.pick-heading h1,
.pick-detail h2 {
  font-size: clamp(27px, 3vw, 46px);
  margin: 0;
}
.pick-heading p {
  margin: 2px 0;
  color: #73634f;
}
.pick-heading select {
  padding: 5px;
  background: #f3e6d2;
  border: 1px solid #b5a289;
}
.pick-import {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 12px;
  font-size: 14px;
}
.pick-layout {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(200px, 29%);
  gap: clamp(12px, 2vw, 32px);
  margin-top: 14px;
}
.pick-roster nav {
  margin-bottom: 12px;
  gap: 6px;
}
.pick-roster nav button {
  border-bottom: 2px solid transparent;
  padding: 3px 10px;
}
.pick-roster nav button[aria-pressed='true'] {
  color: #902e23;
  border-color: #902e23;
}
.pick-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(78px, 1fr));
  gap: 8px;
  max-height: 64dvh;
  overflow: auto;
  padding: 3px;
}
.pick-grid button {
  position: relative;
  width: 100%;
  aspect-ratio: var(--playing-card-ratio);
  overflow: hidden;
  border: 1px solid #97856c;
  border-radius: 5px;
}
.pick-grid button[aria-pressed='true'] {
  outline: 3px solid #a13e27;
  outline-offset: -2px;
}
.pick-grid img {
  height: 100%;
  width: 100%;
  object-fit: cover;
  object-position: top;
}
.pick-grid button span {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: #291b13bf;
  color: #fff0d5;
  font-size: 15px;
}
.pick-detail {
  align-self: start;
  display: grid;
  grid-template-columns: 1fr;
  gap: 10px;
}
.pick-detail > img {
  max-height: 22dvh;
  margin: auto;
  object-fit: contain;
}
.pick-description {
  max-height: 29dvh;
  overflow: auto;
}
.pick-description p {
  font-size: 14px;
  margin: 4px 0;
  line-height: 1.5;
}
.pick-description b {
  color: #853626;
}
@media (max-height: 500px) {
  .pick-page {
    padding: 8px 20px;
  }
  .pick-heading {
    padding-bottom: 4px;
  }
  .pick-detail > img {
    display: none;
  }
  .pick-grid {
    max-height: 57dvh;
  }
  .pick-description {
    max-height: 41dvh;
  }
  .pick-grid button {
    width: min(100%, calc(94px * var(--playing-card-ratio)));
    justify-self: center;
  }
}
</style>
