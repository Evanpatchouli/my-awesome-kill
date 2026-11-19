import { ref, shallowRef } from 'vue'
import { defineStore } from 'pinia'
import type { Duel } from './generated/duel/duel.js'
import { saveMatch, type MatchRecord as RecordToSave } from './persistence'
import {
  describeEvent,
  sides,
  generalById,
  visualEvents,
  type Action,
  type TableView,
} from './classicProtocol'
import { settleDelay, type FeedbackEvent } from './battleMotion'

let bindings: Promise<typeof import('./generated/duel/duel.js')> | undefined
function loadEngine() {
  bindings ??= import('./generated/duel/duel.js')
    .then(async (mod) => {
      await mod.default()
      return mod
    })
    .catch((err) => {
      bindings = undefined
      throw err
    })
  return bindings
}

export const useGame = defineStore('game', () => {
  let match: Duel | undefined
  let generation = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  let matchId = ''
  const view = shallowRef<TableView | null>(null)
  const feedback = shallowRef<{ revision: number; events: FeedbackEvent[] }>({
    revision: 0,
    events: [],
  })
  const log = ref<string[]>([])
  const busy = ref(false)
  const locked = ref(false)
  const error = ref('')
  const saveError = ref(false)
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

  function stop() {
    generation++
    clearTimeout(timer)
    timer = undefined
    match?.free()
    match = undefined
    view.value = null
    feedback.value = { revision: 0, events: [] }
    log.value = []
    locked.value = false
    busy.value = false
  }
  function refresh() {
    if (!match) return
    const current: TableView = JSON.parse(match.view(0))
    view.value = current
    const events = visualEvents(current)
    feedback.value = { revision: current.version, events }
    const additions = current.events.flatMap((e) => describeEvent(current, e) ?? [])
    log.value = [...log.value, ...additions].slice(-80)
    if ((current.winner || current.drawn) && !recorded) {
      recorded = true
      const me = current.seats.find((seat) => seat.id === 0)!
      const victory =
        current.winner === 'court'
          ? me.role === 'lord' || me.role === 'loyalist'
          : current.winner === 'rebels'
            ? me.role === 'rebel'
            : current.winner === 'renegade' && me.alive && me.role === 'renegade'
      pendingRecords.value.push({
        id: matchId,
        finishedAt: new Date().toISOString(),
        winner: current.winner ? sides[current.winner] : '平局',
        summary: `${current.drawn ? '平' : victory ? '胜' : '负'} · ${generalById.get(me.hero)?.name} · ${current.seats.length}人身份局 · 第 ${current.turn?.number ?? 1} 回合`,
      })
      persistQueue()
      void retrySave()
    }
    const token = generation
    locked.value = true
    const isBot = !current.winner && !current.drawn && (
      current.roundSuspended || (current.waitingFor != null && current.waitingFor !== 0)
    )
    const counterOnlyPass =
      current.choice?.counter &&
      current.choice.branches.length === 1 &&
      current.choice.branches[0].id === 0
    const delay =
      current.counterWindow && !events.length && (isBot || counterOnlyPass)
        ? 60
        : Math.max(isBot ? 400 : 0, settleDelay(events))
    timer = setTimeout(() => {
      if (!match || token !== generation || current.version !== view.value?.version) return
      locked.value = false
      if (counterOnlyPass && current.choice) {
        act(
          { type: 'select', event: current.choice.event, branch: 0, cards: [], targets: [] },
          current.version,
        )
      } else if (isBot) {
        try {
          if (!match.bot()) throw new Error('当前电脑座位没有合法决策')
          refresh()
        } catch (e) {
          error.value = String(e)
        }
      }
    }, delay)
  }
  async function start(hero: string, count: number) {
    if (busy.value || locked.value) return false
    stop()
    const token = generation
    error.value = ''
    busy.value = true
    try {
      const { Duel } = await loadEngine()
      if (token !== generation) return false
      match = Duel.classic(Math.floor(Math.random() * 0x7fffffff), 0, hero, count)
      matchId =
        globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`
      recorded = false
      refresh()
      return true
    } catch (e) {
      if (token === generation) {
        stop()
        error.value = '开局失败：' + String(e)
      }
      return false
    } finally {
      if (token === generation) busy.value = false
    }
  }
  function act(action: Action, version: number) {
    if (
      locked.value ||
      !match ||
      !view.value ||
      view.value.waitingFor !== 0 ||
      view.value.version !== version
    )
      return
    try {
      match.submit(JSON.stringify({ version, seat: 0, action }))
      error.value = ''
      refresh()
    } catch (e) {
      error.value = '此操作未执行：' + String(e)
    }
  }
  if (pendingRecords.value.length) void retrySave()
  return {
    view,
    feedback,
    log,
    busy,
    locked,
    error,
    saveError,
    pendingRecords,
    retrySave,
    start,
    act,
    leave: stop,
  }
})
