import { useEffect, useState } from 'react'
import { get } from '@/lib/api'

/** Real colours for SVG gradients; the CSS variables in index.css carry the same values. */
export const STATE_HEX: Record<string, string> = {
  moving: '#60a5fa', built: '#34d399', parked: '#8b8b95', dropped: '#fb7185', open: '#fbbf24', unsorted: '#52525b',
}
export const STATE_NAME: Record<string, string> = { moving: 'In development', built: 'Built', parked: 'Parked', dropped: 'Dropped', open: 'Open idea' }

/** Thread detail (sources with dates, steps) for a list of thread names, fetched in parallel. */
export function useDetails(names: string[]) {
  const [d, setD] = useState<Record<string, any>>({})
  const key = names.join('|')
  useEffect(() => {
    if (!names.length) return
    let live = true
    Promise.all(names.map((n) => get(`/api/thread?name=${encodeURIComponent(n)}`).then((x) => [n, x] as const)))
      .then((r) => { if (live) setD(Object.fromEntries(r)) })
    return () => { live = false }
  }, [key]) // eslint-disable-line react-hooks/exhaustive-deps
  return d
}

export const dayKey = (t?: string) => (t ?? '').slice(0, 10)

/** Sources per day for the last `n` days, oldest first. */
export function dayCounts(detail: any, n: number, kind?: string): number[] {
  const out: number[] = Array(n).fill(0)
  const today = new Date(); today.setHours(12, 0, 0, 0)
  for (const s of detail?.sources ?? []) {
    if (kind && s.kind !== kind) continue
    const k = dayKey(s.first_ts || s.last_ts)
    if (!k) continue
    const age = Math.round((today.getTime() - new Date(k + 'T12:00:00').getTime()) / 864e5)
    if (age >= 0 && age < n) out[n - 1 - age]++
  }
  return out
}

/** The first open step by creation time, as `backtrail brief` picks it. */
export function nextStep(detail: any): string | null {
  const steps = (detail?.items ?? []).filter((i: any) => i.type === 'step' && i.state === 'open')
  steps.sort((a: any, b: any) => String(a.created).localeCompare(String(b.created)))
  return steps[0]?.title ?? null
}

/** What a step's state means and who set it. */
export const STEP_STATUS: Record<string, { label: string; hint: string; color: string }> = {
  open: { label: 'To do', hint: 'Not started', color: '#8b8b95' },
  doing: { label: 'In progress', hint: 'You marked it in progress with Start. Claude does not set this.', color: '#60a5fa' },
  looks_done: { label: 'Built, check it', hint: 'Claude or a commit says it is built. Confirm if it works, or say not yet.', color: '#34d399' },
  done: { label: 'Done', hint: 'You confirmed it.', color: '#34d399' },
}
