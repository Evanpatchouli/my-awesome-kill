import { defineStore } from 'pinia'
import { markRaw, ref } from 'vue'
import { createMatch } from '@mak/rules'
import { settleDelay } from './battleMotion'
import { saveMatch } from './persistence'

type Hero = 'zhangfei' | 'huangyueying'
type Match = ReturnType<typeof createMatch>
export type MatchView = ReturnType<Match['view']>
type RecordToSave = Parameters<typeof saveMatch>[0]

export const useGame = defineStore('game', () => {
  const match = ref<Match | null>(null)
  const view = ref<MatchView | null>(null)
  const waiting = ref(false)
  const locked = ref(false)
  const saveError = ref(false)
  const selectedCard = ref<number | null>(null)
  let timer: ReturnType<typeof setTimeout> | undefined
  let inputTimer: ReturnType<typeof setTimeout> | undefined
  let generation = 0
  const queueKey = 'mak.pending-match-history.v1'
  const pendingRecords = ref<RecordToSave[]>([])
  let saving = false
  let recorded = false
  try {
    const stored = localStorage.getItem(queueKey)
    if (stored) {
      const parsed: unknown = JSON.parse(stored)
      if (Array.isArray(parsed))
        pendingRecords.value = parsed.filter(
          (record): record is RecordToSave =>
            record &&
            typeof record.id === 'string' &&
            typeof record.finishedAt === 'string' &&
            typeof record.winner === 'string' &&
            typeof record.summary === 'string',
        )
    }
  } catch {
    saveError.value = true
  }
  function persistQueue() {
    try {
      localStorage.setItem(queueKey, JSON.stringify(pendingRecords.value))
    } catch {
      saveError.value = true
    }
  }

  function stop() {
    generation++
    if (timer) clearTimeout(timer)
    if (inputTimer) clearTimeout(inputTimer)
    inputTimer = undefined
    timer = undefined
    waiting.value = false
    locked.value = false
  }
  async function retrySave() {
    if (saving) return
    saving = true
    try {
      while (pendingRecords.value.length) {
        const record = pendingRecords.value[0]!
        await saveMatch(record)
        pendingRecords.value = pendingRecords.value.filter((item) => item.id !== record.id)
        persistQueue()
      }
      saveError.value = false
    } catch {
      saveError.value = true
    } finally {
      saving = false
    }
  }
  function refresh() {
    if (!match.value) return
    view.value = match.value.view()
    selectedCard.value = null
    if (view.value.winner !== null && !recorded) {
      const winner = view.value.winner
      recorded = true
      const record: RecordToSave = {
        id: `${Date.now()}-${Math.random().toString(36).slice(2)}`,
        finishedAt: new Date().toISOString(),
        winner: view.value.players[winner].name,
        summary: `${winner === 0 ? '胜' : '负'} · ${view.value.players[0].name} 对阵 ${view.value.players[1].name} · 第 ${view.value.turn} 回合`,
      }
      pendingRecords.value.push(record)
      persistQueue()
      void retrySave()
    }
    const delay = settleDelay(view.value.events)
    lockInput(delay)
    scheduleBot(delay)
  }
  function scheduleBot(delay: number) {
    if (!match.value || !view.value || view.value.winner !== null || view.value.actor !== 1) return
    waiting.value = true
    const token = generation
    const revision = view.value.revision
    timer = setTimeout(() => {
      if (token !== generation || !match.value || view.value?.revision !== revision) return
      waiting.value = false
      if (match.value.bot()) refresh()
    }, Math.max(500, delay + 40))
  }
  function lockInput(delay: number) {
    if (inputTimer) clearTimeout(inputTimer)
    locked.value = true
    const token = generation
    inputTimer = setTimeout(() => {
      if (token === generation) locked.value = false
    }, delay)
  }
  function start(hero: Hero) {
    if (locked.value) return false
    stop()
    recorded = false
    match.value = markRaw(createMatch({ seed: Math.floor(Math.random() * 0x7fffffff), hero }))
    refresh()
    return true
  }
  function act(key: string, revision: number) {
    if (
      !match.value ||
      !view.value ||
      waiting.value ||
      locked.value ||
      view.value.actor !== 0 ||
      revision !== view.value.revision
    )
      return
    if (match.value.act(revision, key)) refresh()
  }
  function discard(ids: number[], revision: number) {
    if (locked.value || waiting.value || !match.value) return
    if (match.value.discard(revision, ids)) refresh()
  }
  function leave() {
    stop()
    match.value = null
    view.value = null
    selectedCard.value = null
  }
  if (pendingRecords.value.length) void retrySave()
  return {
    view,
    waiting,
    locked,
    saveError,
    pendingRecords,
    selectedCard,
    start,
    act,
    discard,
    leave,
    retrySave,
  }
})
