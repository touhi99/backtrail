import { useEffect, useState } from 'react'
import { get } from '@/lib/api'
import type { ThreadDetail } from '@/lib/types'

export const STATE_HEX: Record<string, string> = {
  moving: '#60a5fa', built: '#34d399', parked: '#8b8b95', dropped: '#fb7185', open: '#fbbf24', unsorted: '#52525b',
}
export const STATE_NAME: Record<string, string> = { moving: 'In development', built: 'Built', parked: 'Parked', dropped: 'Dropped', open: 'Open idea' }

export function useDetails(names: string[]) {
  const [d, setD] = useState<Record<string, ThreadDetail>>({})
  const key = names.join('|')
  useEffect(() => {
    const wanted = key ? key.split('|') : []
    if (!wanted.length) return
    let live = true
    Promise.all(wanted.map((n) => get<ThreadDetail>(`/api/thread?name=${encodeURIComponent(n)}`).then((x) => [n, x] as const)))
      .then((r) => { if (live) setD(Object.fromEntries(r)) })
    return () => { live = false }
  }, [key])
  return d
}

export const dayKey = (t?: string) => (t ?? '').slice(0, 10)

export function dayCounts(detail: ThreadDetail | undefined, n: number, kind?: string): number[] {
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

export function nextStep(detail: ThreadDetail | undefined): string | null {
  const steps = (detail?.items ?? []).filter((i) => i.type === 'step' && i.state === 'open')
  steps.sort((a, b) => String(a.created).localeCompare(String(b.created)))
  return steps[0]?.title ?? null
}

export const STEP_STATUS: Record<string, { label: string; hint: string; color: string }> = {
  open: { label: 'To do', hint: 'Not started', color: '#8b8b95' },
  doing: { label: 'In progress', hint: 'You marked it in progress with Start. Claude does not set this.', color: '#60a5fa' },
  looks_done: { label: 'Built, check it', hint: 'Claude or a commit says it is built. Confirm if it works, or say not yet.', color: '#34d399' },
  done: { label: 'Done', hint: 'You confirmed it.', color: '#34d399' },
}
