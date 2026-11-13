import { useEffect, useState, useCallback } from 'react'

export async function get<T = any>(url: string): Promise<T> { return (await fetch(url)).json() }
export async function post<T = any>(url: string, body: unknown): Promise<T> {
  return (await fetch(url, { method: 'POST', body: JSON.stringify(body) })).json()
}

export function useApi<T = any>(url: string | null) {
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

/** A suggestion's staleness label, set by `backtrail flag`: "kind|reason". */
export const STALE_LABEL: Record<string, string> = { in_effect: 'Already in effect', superseded: 'Replaced later', done: 'Already done' }
export const staleOf = (i: any): { kind: string; reason: string } | null => {
  const [kind, ...r] = (i?.stale ?? '').split('|')
  return kind && kind !== 'live' && STALE_LABEL[kind] ? { kind, reason: r.join('|') } : null
}

export const dayFmt = (t?: string) => {
  if (!t) return ''
  const d = new Date(t.slice(0, 10) + 'T12:00:00')
  return d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short' })
}
export const agoFmt = (t?: string) => {
  if (!t) return ''
  const n = Math.round((Date.now() - new Date(t.slice(0, 10) + 'T12:00:00').getTime()) / 864e5)
  return n <= 0 ? 'today' : n === 1 ? 'yesterday' : `${n} days ago`
}

/** Who a stored actor/origin value is, as shown in the app. */
export const WHO: Record<string, string> = { owner: 'You', claude: 'Claude', suggested: 'Claude', compiled: 'Claude' }
export const whoOrigin = (i: any) => (i?.origin === 'owner' ? 'You' : 'Claude')
export const whoActor = (i: any) => WHO[i?.by_actor] ?? whoOrigin(i)
