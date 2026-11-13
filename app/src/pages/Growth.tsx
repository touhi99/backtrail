import { useMemo, useState, type ReactNode } from 'react'
import { dayFmt, useApi } from '@/lib/api'
import Paged from '@/components/Paged'
import '@/next.css'

type Row = { id: string; d: string; sid: number; stitle: string; skind: string; es: string | null }
const PAL = ['#60a5fa', '#34d399', '#fbbf24', '#c084fc', '#fb7185', '#2dd4bf', '#f97316', '#a3e635']
const OTHER = '#52525b'
const DAY = 86400000
const wk = (d: string) => { const t = new Date(d + 'T00:00:00Z'); const dow = (t.getUTCDay() + 6) % 7; return new Date(t.getTime() - dow * DAY).toISOString().slice(0, 10) }
const nice = (d: string) => new Date(d + 'T00:00:00Z').toLocaleDateString('en-GB', { day: 'numeric', month: 'short', timeZone: 'UTC' })

/** Growth of the graph over time. By topic: cumulative claims per entity, week by week. By session: what each session added. */
export default function Growth({ nodes, tabs }: { nodes: any[]; tabs: ReactNode }) {
  const { data } = useApi('/api/kg/growth') as { data: Row[] | null }
  const [mode, setMode] = useState<'topic' | 'session'>('topic')
  const [hover, setHover] = useState(-1)
  const [allTopics, setAllTopics] = useState(false)
  const kindOf = useMemo(() => new Map<string, string>(nodes.map((n) => [n.id, n.kind])), [nodes])
  const name = useMemo(() => new Map<string, string>(nodes.map((n) => [n.id, n.name.replace(/ \(.*\)/, '')])), [nodes])
  const rows = useMemo(() => (data ?? []).filter((r) => r.d), [data])

  const topic = useMemo(() => {
    if (!rows.length) return null
    const span = (new Date(rows[rows.length - 1].d).getTime() - new Date(rows[0].d).getTime()) / DAY
    const daily = span <= 70, bk = (d: string) => (daily ? d : wk(d))
    const first = bk(rows[0].d), last = bk(rows[rows.length - 1].d)
    const weeks: string[] = []
    for (let t = new Date(first + 'T00:00:00Z').getTime(); t <= new Date(last + 'T00:00:00Z').getTime(); t += (daily ? 1 : 7) * DAY) weeks.push(new Date(t).toISOString().slice(0, 10))
    const per = new Map<string, number[]>(), seen = new Map<string, string>()
    for (const r of rows) for (const e of (r.es ?? '').split(',').filter(Boolean)) {
      if (!per.has(e)) per.set(e, Array(weeks.length).fill(0))
      per.get(e)![weeks.indexOf(bk(r.d))]++
      if (!seen.has(e) || r.d < seen.get(e)!) seen.set(e, r.d)
    }
    const total = (e: string) => per.get(e)!.reduce((a, b) => a + b, 0)
    const ids = [...per.keys()].sort((a, b) => total(b) - total(a))
    const top = ids.slice(0, 8), rest = ids.slice(8)
    const series = top.map((e, i) => ({ id: e, label: name.get(e) ?? e, color: PAL[i], v: per.get(e)! }))
    if (rest.length) series.push({ id: 'other', label: `Other (${rest.length})`, color: OTHER, v: weeks.map((_, w) => rest.reduce((a, e) => a + per.get(e)![w], 0)) })
    const cum = series.map((s) => { let a = 0; return s.v.map((x) => (a += x)) })
    const sum = weeks.map((_, w) => cum.reduce((a, c) => a + c[w], 0))
    const firsts = [...seen.entries()].sort((a, b) => a[1].localeCompare(b[1]))
    const step = (m: number) => (m <= 100 ? 20 : m <= 300 ? 50 : m <= 800 ? 200 : 500)
    const mx = Math.max(...sum, 1), st = step(mx)
    return { daily, weeks, series, cum, sum, max: Math.ceil(mx / st) * st, step: st, firsts, per }
  }, [rows, name])

  const sess = useMemo(() => {
    const m = new Map<number, { sid: number; title: string; kind: string; d: string; n: number; ents: Set<string> }>()
    for (const r of rows) {
      const s = m.get(r.sid) ?? { sid: r.sid, title: r.stitle, kind: r.skind, d: r.d, n: 0, ents: new Set<string>() }
      s.n++; if (r.d < s.d) s.d = r.d
      for (const e of (r.es ?? '').split(',').filter(Boolean)) s.ents.add(e)
      m.set(r.sid, s)
    }
    const list = [...m.values()].sort((a, b) => a.d.localeCompare(b.d) || a.sid - b.sid)
    const known = new Set<string>(); let cum = 0
    const out = list.map((s) => { const fresh = [...s.ents].filter((e) => !known.has(e)); s.ents.forEach((e) => known.add(e)); cum += s.n; return { ...s, fresh, cum } })
    return out
  }, [rows])

  const Chart = () => {
    if (!topic) return null
    const W = 900, H = 320, L = 58, R = 12, T = 12, B = 30, n = topic.weeks.length
    const x = (i: number) => L + (n === 1 ? (W - L - R) / 2 : (i / (n - 1)) * (W - L - R))
    const y = (v: number) => T + (1 - v / topic.max) * (H - T - B)
    const ticks = Array.from({ length: Math.round(topic.max / topic.step) + 1 }, (_, i) => i * topic.step)
    let below = topic.weeks.map(() => 0)
    const areas = topic.series.map((s, k) => {
      const top = topic.cum[k].map((v, i) => below[i] + v)
      const d = top.map((v, i) => `${i ? 'L' : 'M'}${x(i)},${y(v)}`).join(' ') + below.map((v, i) => `L${x(n - 1 - i)},${y(below[n - 1 - i])}`).join(' ') + 'Z'
      below = top
      return <path key={s.id} d={d} fill={s.color} opacity={0.85} />
    })
    return (
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full" role="img" aria-label="Cumulative claims per topic by week" onMouseLeave={() => setHover(-1)}>
        {ticks.map((t) => <g key={t}><line x1={L} x2={W - R} y1={y(t)} y2={y(t)} stroke="#26262b" /><text x={L - 8} y={y(t) + 4} textAnchor="end" fontSize="11" fill="#8b8b95">{t}</text></g>)}
        {areas}
        <text transform={`rotate(-90) translate(${-(T + (H - T - B) / 2)},12)`} textAnchor="middle" fontSize="11" fill="#8b8b95">claims so far</text>
        {topic.weeks.map((w, i) => (i % Math.ceil(n / 8) === 0 || i === n - 1) && <text key={w} x={x(i)} y={H - 8} textAnchor={i === n - 1 ? 'end' : i === 0 ? 'start' : 'middle'} fontSize="11" fill="#8b8b95">{nice(w)}</text>)}
        {topic.weeks.map((w, i) => <rect key={w} x={x(i) - (W - L - R) / Math.max(n - 1, 1) / 2} y={T} width={(W - L - R) / Math.max(n - 1, 1)} height={H - T - B} fill="transparent" onMouseEnter={() => setHover(i)} />)}
        {hover >= 0 && <line x1={x(hover)} x2={x(hover)} y1={T} y2={H - B} stroke="#fff" strokeOpacity=".5" />}
      </svg>
    )
  }

  return (
    <div className="nx-page">
      <div className="nx-in flex flex-wrap items-end justify-between gap-3">
        <div><div className="text-[13px] text-mut">How the graph grew from the first claim to today</div>
          <div className="flex flex-wrap items-center gap-4"><h2 className="text-[28px] font-semibold leading-tight tracking-tight">Graph</h2>{tabs}</div></div>
        <div className="flex gap-1.5">
          {([['topic', 'By topic'], ['session', 'By session']] as const).map(([k, l]) => (
            <button key={k} type="button" onClick={() => setMode(k)} className="rounded-full px-3 text-[12.5px] font-medium" style={{ height: 28, border: '1px solid var(--color-line2)', background: mode === k ? '#34d39926' : 'rgba(255,255,255,.04)', color: mode === k ? '#34d399' : 'var(--color-soft)' }}>{l}</button>
          ))}
        </div>
      </div>
      {!data && <div className="mt-6 text-[13px] text-mut">Loading…</div>}
      {data && rows.length === 0 && <div className="nx-panel mt-4 px-5 py-8 text-center text-[13.5px] text-mut">No claims with a date yet.</div>}

      {topic && mode === 'topic' && (
        <>
          <p className="mt-3 max-w-[70ch] text-[13.5px] text-soft">Claims added so far, stacked by the entity they are about, one step per {topic.daily ? 'day' : 'week'}. The height of the stack at any date is the number of claims recorded up to then, split by what they are about. A claim about two entities counts for both. The 8 largest entities have their own colour.</p>
          <section className="nx-panel nx-in mt-3 px-4 py-3">
            <div className="min-h-[20px] text-[12.5px] text-soft">{hover >= 0 ? <>{topic.daily ? '' : 'Week of '}{nice(topic.weeks[hover])}: {topic.series.map((s, k) => topic.series[k].v[hover] > 0 ? `${s.label} +${s.v[hover]}` : '').filter(Boolean).join(' · ') || 'nothing new'}</> : `Hover the chart to see what each ${topic.daily ? 'day' : 'week'} added.`}</div>
            <Chart />
            <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[12px] text-mut">{topic.series.map((s) => <span key={s.id} className="flex items-center gap-1.5"><span className="size-2 rounded-full" style={{ background: s.color }} />{s.label} <span className="tabular-nums">{s.v.reduce((a, b) => a + b, 0)}</span></span>)}</div>
          </section>
          <Lanes topic={topic} name={name} kind={kindOf} all={allTopics} setAll={setAllTopics} />
        </>
      )}

      {mode === 'session' && sess.length > 0 && (
        <section className="nx-panel nx-in mt-4 overflow-hidden">
          <p className="px-5 pb-1 pt-4 text-[13px] text-soft">Each session or note that a claim was taken from, newest first: how many claims it added, which topics it introduced for the first time, and the running total.</p>
          <Paged items={[...sess].reverse()} size={15}>{(pageRows: any[]) => pageRows.map((s) => (
            <div key={s.sid} className="nx-row" style={{ gridTemplateColumns: '54px minmax(0,1fr) 140px 64px', cursor: 'default' }}>
              <span className="text-[12.5px] tabular-nums text-mut">{dayFmt(s.d)}</span>
              <div className="min-w-0">
                <div className="truncate text-[13.5px] font-medium">{s.title || 'Untitled'} <span className="text-[11.5px] font-normal text-mut">{s.kind}</span></div>
                {s.fresh.length > 0 && <div className="truncate text-[12px] text-open">New topic: {s.fresh.map((e) => name.get(e) ?? e).join(', ')}</div>}
              </div>
              <div className="flex items-center gap-2"><span className="h-2 rounded-full bg-accent" style={{ width: Math.max(4, s.n * 8), maxWidth: 100 }} /><span className="text-[12.5px] tabular-nums text-soft">+{s.n}</span></div>
              <span className="text-right text-[12.5px] tabular-nums text-mut">{s.cum}</span>
            </div>
          ))}</Paged>
        </section>
      )}
    </div>
  )
}

const KIND_C: Record<string, string> = { project: '#60a5fa', service: '#c084fc', tool: '#fbbf24', site: '#34d399', topic: '#f472b6' }

/** One row per entity, oldest first: a dot where it first appears and a bar per week for how many claims it got. */
function Lanes({ topic, name, kind, all, setAll }: { topic: any; name: Map<string, string>; kind: Map<string, string>; all: boolean; setAll: (f: (v: boolean) => boolean) => void }) {
  const n = topic.weeks.length, W = 900, L = 170, R = 12, RH = 22, T = 8, B = 24, bw = Math.max(2, Math.min(10, ((W - L - R) / n) * 0.7)), u = topic.daily ? 'day' : 'week'
  const list: [string, string][] = all ? topic.firsts : topic.firsts.slice(0, 20)
  const x = (i: number) => L + (n === 1 ? (W - L - R) / 2 : (i / (n - 1)) * (W - L - R))
  const cell = Math.max(1, ...[...topic.per.values()].flatMap((v: number[]) => v))
  const H = T + list.length * RH + B
  return (
    <section className="nx-panel nx-in mt-4 px-4 py-4">
      <h3 className="text-[15px] font-semibold">When each topic appeared and how active it was</h3>
      <p className="mt-1 text-[12.5px] text-mut">One row per entity, oldest first. The dot marks its first claim; each bar is one {u}, taller for more claims.</p>
      <svg viewBox={`0 0 ${W} ${H}`} className="mt-2 w-full" role="img" aria-label="First appearance and weekly activity of each entity">
        {topic.weeks.map((w: string, i: number) => <line key={w} x1={x(i)} x2={x(i)} y1={T} y2={H - B} stroke="#1d1d21" />)}
        {list.map(([e, d]: [string, string], r: number) => {
          const c = KIND_C[kind.get(e) ?? ''] ?? '#8b8b95', v: number[] = topic.per.get(e), y0 = T + r * RH + RH - 3
          const fi = v.findIndex((q) => q > 0)
          return (
            <g key={e}>
              <text x={L - 10} y={y0 + 3} textAnchor="end" fontSize="12" fill="#d4d4d8">{((name.get(e) ?? e).length > 24 ? (name.get(e) ?? e).slice(0, 23) + '…' : (name.get(e) ?? e))}</text>
              <line x1={x(fi)} x2={W - R} y1={y0} y2={y0} stroke={c} strokeOpacity=".25" />
              {v.map((q, i) => q > 0 && <rect key={i} x={x(i) - bw / 2} width={bw} y={y0 - Math.max(2, (q / cell) * 15)} height={Math.max(2, (q / cell) * 15)} fill={c} opacity=".9" rx="1.5"><title>{`${name.get(e) ?? e}, ${u === 'day' ? '' : 'week of '}${nice(topic.weeks[i])}: ${q} claims`}</title></rect>)}
              <circle cx={x(fi)} cy={y0 - 1} r="3.5" fill={c} stroke="#fff" strokeWidth="1"><title>{`First claim ${nice(d)}`}</title></circle>
            </g>
          )
        })}
        {topic.weeks.map((w: string, i: number) => (i % Math.ceil(n / 8) === 0 || i === n - 1) && <text key={w} x={x(i)} y={H - 6} textAnchor={i === n - 1 ? 'end' : i === 0 ? 'start' : 'middle'} fontSize="11" fill="#8b8b95">{nice(w)}</text>)}
      </svg>
      {topic.firsts.length > 20 && <button type="button" className="mt-1 text-[13px] text-mut hover:text-ink" onClick={() => setAll((v) => !v)}>{all ? 'Show fewer' : `Show all ${topic.firsts.length}`}</button>}
    </section>
  )
}
