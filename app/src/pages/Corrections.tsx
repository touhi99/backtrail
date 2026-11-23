import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { useApi } from '@/lib/api'
import { Ring, Spark } from '@/components/Viz'
import type { CorrectionMessages, Corrections as Data } from '@/lib/types'
import '@/next.css'

const KIND: Record<string, { label: string; color: string; hint: string }> = {
  misread: { label: 'Misread you', color: '#c084fc', hint: 'Misunderstood the request or lost context it had' },
  unchecked: { label: 'Didn’t check', color: '#fbbf24', hint: 'Said it was done or working without checking' },
  off_target: { label: 'Off target', color: '#fb7185', hint: 'Not what was wanted, or not good enough' },
  overreach: { label: 'Did too much', color: '#fb923c', hint: 'Did or planned something not asked for' },
  too_much: { label: 'Too long', color: '#2dd4bf', hint: 'Too much text, too many steps or too costly' },
  unresearched: { label: 'Didn’t research', color: '#38bdf8', hint: 'Answered from memory instead of checking' },
}
const ORDER = Object.keys(KIND)
const pct = (n: number, d: number) => (d ? Math.round((n * 100) / d) : 0)
const dayShort = (w: string) => new Date(w + 'T12:00:00').toLocaleDateString('en-GB', { day: 'numeric', month: 'short' })

type Thread = Data['threads'][number]
type Filter = { kind?: string; thread?: string; week?: string }

function treemapRows(items: Thread[], rows = 3): Thread[][] {
  const total = items.reduce((a, t) => a + t.turns, 0)
  const out: Thread[][] = []
  let cur: Thread[] = [], sum = 0
  for (const t of items) {
    cur.push(t); sum += t.turns
    if (sum >= (total / rows) * (out.length + 1) && out.length < rows - 1) { out.push(cur); cur = [] }
  }
  if (cur.length) out.push(cur)
  return out
}

function heat(rate: number) {
  const a = Math.min(1, Math.max(0, (rate - 30) / 45))
  return `rgba(251, ${Math.round(191 - a * 80)}, ${Math.round(36 + a * 60)}, ${0.18 + a * 0.62})`
}

export default function Corrections() {
  const { data } = useApi<Data>('/api/corrections')
  const [f, setF] = useState<Filter>({})
  const panel = useRef<HTMLElement>(null)
  const active = Boolean(f.kind || f.thread || f.week)
  const qs = new URLSearchParams({ kind: f.kind ?? '', thread: f.thread ?? '', week: f.week ?? '' }).toString()
  const { data: msgs } = useApi<CorrectionMessages>(active ? `/api/corrections/messages?${qs}` : null)
  useEffect(() => {
    if (active) panel.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
  }, [active, qs])
  const toggle = (next: Filter) => setF((cur) => {
    const same = Object.entries(next).every(([k, v]) => cur[k as keyof Filter] === v)
    return same ? Object.fromEntries(Object.keys(cur).filter((k) => !(k in next)).map((k) => [k, cur[k as keyof Filter]])) : { ...cur, ...next }
  })
  if (!data) return null
  const weeks = data.weeks
  const now = weeks[weeks.length - 1], prev = weeks[weeks.length - 2]
  const rate = now ? pct(now.corrections, now.turns) : 0
  const delta = prev ? rate - pct(prev.corrections, prev.turns) : 0
  const kindTotal = data.kinds.reduce((a, k) => a + k.n, 0)
  const kindWeeks = weeks.filter((w) => Object.values(w.kinds).some((n) => n > 0))

  if (weeks.length === 0) {
    return (
      <div className="nx-page"><div className="relative mx-auto max-w-[1080px]">
        <h2 className="nx-in text-[28px] font-semibold tracking-tight">Corrections</h2>
        <section className="nx-panel nx-in mt-6 px-6 py-8 text-[14px] text-soft">Nothing tagged yet. Run <code className="rounded bg-white/5 px-1.5">skill/corrections.sh</code>; the sweep keeps it current.</section>
      </div></div>
    )
  }

  return (
    <div className="nx-page">
      <div className="pointer-events-none absolute inset-x-0 top-0 h-[420px]" style={{ background: 'radial-gradient(900px 420px at 30% -80px, rgba(251,191,36,.07), transparent 70%)' }} />
      <div className="relative mx-auto flex max-w-[1080px] flex-col gap-5">
        <h2 className="nx-in text-[28px] font-semibold leading-tight tracking-tight">Corrections</h2>

        <section className="nx-in grid gap-4 md:grid-cols-[300px_1fr]" style={{ '--d': '40ms' }}>
          <div className="nx-panel flex flex-col items-center gap-3 p-5" title="Share of your messages that pushed back on the agent this week">
            <span className="text-[13px] text-mut">This week</span>
            <Ring value={rate / 100} size={168} stroke={14} color="#fbbf24">
              <div className="text-[40px] font-semibold tabular-nums tracking-tight">{rate}%</div>
              <div className="text-[12.5px] text-mut">pushed back</div>
            </Ring>
            {prev && <span className="text-[13px] font-semibold" style={{ color: delta <= 0 ? '#34d399' : '#fb7185' }}>{delta <= 0 ? '▼' : '▲'} {Math.abs(delta)} pts</span>}
            <Spark data={weeks.map((w) => pct(w.corrections, w.turns))} color="#fbbf24" h={46} />
          </div>

          <div className="nx-panel flex min-w-0 flex-col p-5">
            <span className="text-[13px] text-mut">What goes wrong</span>
            <div className="mt-3 flex h-[200px] items-end gap-2.5">
              {kindWeeks.map((w) => {
                const n = ORDER.reduce((a, k) => a + (w.kinds[k] ?? 0), 0) || 1
                return (
                  <div key={w.week} className="flex h-full flex-1 flex-col-reverse gap-[2px] overflow-hidden rounded-lg">
                    {ORDER.map((k) => {
                      const lit = (!f.kind || f.kind === k) && (!f.week || f.week === w.week)
                      return (
                        <button key={k} type="button" title={`${KIND[k].label}, week of ${dayShort(w.week)}: ${pct(w.kinds[k] ?? 0, n)}%`}
                          onClick={() => toggle({ kind: k, week: w.week })} className="block w-full cursor-pointer transition-opacity hover:brightness-125"
                          style={{ height: `${pct(w.kinds[k] ?? 0, n)}%`, background: KIND[k].color, opacity: lit ? 1 : 0.18 }} />
                      )
                    })}
                  </div>
                )
              })}
            </div>
            <div className="mt-2 flex gap-2.5">{kindWeeks.map((w) => (
              <button key={w.week} type="button" onClick={() => toggle({ week: w.week })}
                className={`flex-1 rounded-md py-0.5 text-center text-[11.5px] transition hover:bg-white/5 ${f.week === w.week ? 'font-semibold text-ink' : 'text-mut'}`}>{dayShort(w.week)}</button>
            ))}</div>
            <div className="mt-4 flex flex-wrap gap-2">
              {data.kinds.filter((k) => k.n > 0).sort((a, b) => b.n - a.n).map((k) => (
                <button key={k.kind} type="button" title={KIND[k.kind].hint} onClick={() => toggle({ kind: k.kind })}
                  className={`inline-flex h-8 items-center gap-2 rounded-[10px] border px-3 text-[13px] transition ${f.kind === k.kind ? 'border-[#45454d] bg-white/10 text-ink' : 'border-line2 bg-white/[.04] text-soft hover:bg-white/[.08]'}`}>
                  <span className="size-2.5 rounded-[3px]" style={{ background: KIND[k.kind].color }} />{KIND[k.kind].label}<b className="tabular-nums text-ink">{pct(k.n, kindTotal)}%</b>
                </button>
              ))}
            </div>
            <span className="mt-3 text-[12px] text-mut">Click a bar, a week, a kind or a box below to see those messages</span>
          </div>
        </section>

        <section className="nx-panel nx-in flex flex-col gap-3 p-5" style={{ '--d': '80ms' }}>
          <span className="text-[13px] text-mut" title="Box size is how many messages; colour is how often they pushed back">Where</span>
          <div className="flex h-[300px] flex-col gap-1.5">
            {treemapRows(data.threads).map((row, i) => (
              <div key={i} className="flex min-h-0 gap-1.5" style={{ flex: row.reduce((a, t) => a + t.turns, 0) }}>
                {row.map((t) => (
                  <button key={t.thread} type="button" onClick={() => toggle({ thread: t.thread })} title={`${pct(t.corrections, t.turns)}% of ${t.turns} messages`}
                    className={`flex min-w-0 flex-col items-start justify-end overflow-hidden rounded-xl p-3 text-left transition hover:brightness-125 ${f.thread === t.thread ? 'ring-2 ring-white/80' : ''}`}
                    style={{ flex: t.turns, background: heat(pct(t.corrections, t.turns)), opacity: f.thread && f.thread !== t.thread ? 0.45 : 1 }}>
                    <span className="w-full truncate text-[13px] font-semibold text-ink">{t.thread}</span>
                    <span className="text-[12px] tabular-nums text-soft">{pct(t.corrections, t.turns)}%</span>
                  </button>
                ))}
              </div>
            ))}
          </div>
        </section>

        {active && (
          <section ref={panel} className="nx-panel nx-in flex flex-col gap-3 p-5">
            <div className="flex flex-wrap items-center gap-2">
              {f.kind && <Chip color={KIND[f.kind]?.color} label={KIND[f.kind]?.label ?? f.kind} clear={() => setF({ ...f, kind: undefined })} />}
              {f.thread && <Chip label={f.thread} clear={() => setF({ ...f, thread: undefined })} />}
              {f.week && <Chip label={`week of ${dayShort(f.week)}`} clear={() => setF({ ...f, week: undefined })} />}
              <span className="text-[13px] tabular-nums text-mut">{msgs ? `${msgs.n} message${msgs.n === 1 ? '' : 's'}` : '…'}</span>
              <button type="button" onClick={() => setF({})} title="Clear" className="ml-auto grid size-7 place-items-center rounded-full bg-white/[.06] text-soft hover:bg-white/10">×</button>
            </div>
            <div className="flex max-h-[480px] flex-col gap-1 overflow-y-auto">
              {msgs?.items.map((q) => (
                <Link key={q.chunk_id} to={`/source/${q.source_id}?c=${q.chunk_id}`}
                  className="flex gap-3 rounded-lg px-2 py-2 text-[14px] leading-snug text-soft transition hover:bg-white/[.04] hover:text-ink">
                  <span className="w-[3px] shrink-0 rounded-full" style={{ background: (q.kind && KIND[q.kind]?.color) || '#52525b' }} />
                  <span className="min-w-0 flex-1">{q.text}<span className="ml-2 text-[12px] text-mut">{q.thread} · {dayShort(q.ts.slice(0, 10))}</span></span>
                </Link>
              ))}
              {msgs && msgs.n === 0 && <span className="px-2 py-3 text-[13px] text-mut">No pushback matches all of these. Remove a filter to widen it.</span>}
              {msgs && msgs.n > msgs.items.length && <span className="px-2 pt-1 text-[12px] text-mut">Latest {msgs.items.length} of {msgs.n}</span>}
            </div>
          </section>
        )}
      </div>
    </div>
  )
}

function Chip({ label, color, clear }: { label: string; color?: string; clear: () => void }) {
  return (
    <span className="inline-flex h-8 items-center gap-2 rounded-[10px] border border-line2 bg-white/[.06] pl-3 pr-1.5 text-[13px] font-medium text-ink">
      {color && <span className="size-2.5 rounded-[3px]" style={{ background: color }} />}
      {label}
      <button type="button" onClick={clear} title="Remove" className="grid size-5 place-items-center rounded-full text-mut hover:bg-white/10 hover:text-ink">×</button>
    </span>
  )
}
