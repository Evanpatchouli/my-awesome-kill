import { invoke, isTauri } from '@tauri-apps/api/core'

export interface MatchRecord {
  id: string
  finishedAt: string
  winner: string
  summary: string
}

const storageKey = 'mak.match-history.v1'

/** Save once per match ID, including when the completion event is repeated. */
export async function saveMatch(record: MatchRecord): Promise<void> {
  if (isTauri()) {
    await invoke('save_match', { record })
    return
  }
  const matches = await listMatches()
  if (!matches.some((match) => match.id === record.id)) {
    localStorage.setItem(storageKey, JSON.stringify([record, ...matches]))
  }
}

export async function listMatches(): Promise<MatchRecord[]> {
  if (isTauri()) return invoke<MatchRecord[]>('list_matches')
  const stored = localStorage.getItem(storageKey)
  return stored ? JSON.parse(stored) as MatchRecord[] : []
}
