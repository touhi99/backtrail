import { useEffect, useState, useCallback } from 'react'
import type { Item } from '@/lib/types'

export async function get<T>(url: string): Promise<T> { return (await fetch(url)).json() }
export async function post<T>(url: string, body: unknown): Promise<T> {
  return (await fetch(url, { method: 'POST', body: JSON.stringify(body) })).json()
}

export function useApi<T>(url: string | null) {
  const [data, setData] = useState<T | null>(null)
  const [tick, setTick] = useState(0)
  useEffect(() => {
    if (!url) return
    let live = true
    get<T>(url).then((d) => live && setData(d))
    return () => { live = false }
  }, [url, tick])
  const reload = useCallback(() => setTick((t) => t + 1), [])
  return { data, reload }
}

export const STATE: Record<string, { label: string; color: string }> = {
  moving: { label: 'In development', color: 'var(--color-moving)' },
  built: { label: 'Built', color: 'var(--color-built)' },
  parked: { label: 'Parked', color: 'var(--color-parked)' },
  dropped: { label: 'Dropped', color: 'var(--color-dropped)' },
  open: { label: 'Open idea', color: 'var(--color-open)' },
  unsorted: { label: 'Unsorted', color: '#52525b' },
}

export const STALE_LABEL: Record<string, string> = { in_effect: 'Already in effect', superseded: 'Replaced later', done: 'Already done' }
export const staleOf = (i: Pick<Item, 'stale'> | null | undefined): { kind: string; reason: string } | null => {
  const [kind, ...r] = (i?.stale ?? '').split('|')
  return kind && kind !== 'live' && STALE_LABEL[kind] ? { kind, reason: r.join('|') } : null
}

export const dayFmt = (t?: string | null) => {
  if (!t) return ''
  const d = new Date(t.slice(0, 10) + 'T12:00:00')
  return d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short' })
}
export const agoFmt = (t?: string | null) => {
  if (!t) return ''
  const n = Math.round((Date.now() - new Date(t.slice(0, 10) + 'T12:00:00').getTime()) / 864e5)
  return n <= 0 ? 'today' : n === 1 ? 'yesterday' : `${n} days ago`
}

export const WHO: Record<string, string> = { owner: 'You', claude: 'Claude', suggested: 'Claude', compiled: 'Claude' }
export const whoOrigin = (i: Pick<Item, 'origin'> | null | undefined) => (i?.origin === 'owner' ? 'You' : 'Claude')
export const whoActor = (i: Pick<Item, 'origin' | 'by_actor'> | null | undefined) => WHO[i?.by_actor ?? ''] ?? whoOrigin(i)
