import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { post, useApi } from '@/lib/api'
import { STATE_HEX, STATE_NAME } from '@/lib/next'
import '@/next.css'

type Edge = { a: string; b: string; kind: string; quote?: string; ts?: string; state?: string; source?: string; source_title?: string; source_id?: number }

const GUTTER = 210, TOP = 56, LANE = 34, PAD_R = 40
const day = (t?: string) => (t ?? '').slice(0, 10)

/** Lineage: one lane per project on a time axis, one arrow per relation, dated at the moment it was said. */
export default function MapOverview() {
  const nav = useNavigate()
  const { data: g, reload } = useApi('/api/graph') as { data: any; reload?: () => void }
  const box = useRef<HTMLDivElement>(null)
  const [w, setW] = useState(1030)
  const [sel, setSel] = useState<Edge | null>(null)
  const [hover, setHover] = useState<string | null>(null)

  useEffect(() => {
    const el = box.current; if (!el) return
    const ro = new ResizeObserver(() => setW(el.clientWidth)); ro.observe(el); setW(el.clientWidth)
    return () => ro.disconnect()
  }, [])

  const m = useMemo(() => {
    if (!g) return null
    const threads: any[] = g.nodes.filter((n: any) => n.kind === 'thread' && n.first)
    const rel: Edge[] = g.edges.filter((e: any) => ['spun out of', 'forked from', 'built on', 'replaced by'].includes(e.kind) && e.a.startsWith('T:') && e.b.startsWith('T:'))
    const linked = new Set(rel.flatMap((e) => [e.a, e.b]))
    const lanes = [...threads.filter((t) => linked.has(t.id)), ...threads.filter((t) => !linked.has(t.id))]
      .sort((x, y) => Number(!linked.has(x.id)) - Number(!linked.has(y.id)) || x.first.localeCompare(y.first))
    const t0 = Math.min(...threads.map((t) => +new Date(t.first))), t1 = Math.max(...threads.map((t) => +new Date(t.last)), Date.now())
    return { lanes, rel, linked, t0, t1 }
  }, [g])

  if (!m) return <div ref={box} className="nx-map" />
  const x = (iso: string) => GUTTER + ((+new Date(iso) - m.t0) / (m.t1 - m.t0 || 1)) * (w - GUTTER - PAD_R)
  const y = (id: string) => TOP + m.lanes.findIndex((l) => l.id === id) * LANE + LANE / 2
  const h = TOP + m.lanes.length * LANE + 60
  const ticks = Array.from({ length: 7 }, (_, i) => new Date(m.t0 + ((m.t1 - m.t0) * i) / 6).toISOString())
  const name = (id: string) => id.replace(/^T:/, '')
  const origin = (e: Edge) => (e.kind === 'replaced by' ? e.a : e.b)
  const target = (e: Edge) => (e.kind === 'replaced by' ? e.b : e.a)
  const lit = (e: Edge) => !hover || hover === e.a || hover === e.b

  const review = async (accept: boolean) => {
    if (!sel) return
    await post('/api/relation', { a: sel.a, b: sel.b, kind: sel.kind, accept })
    setSel(null); reload?.()
  }

  return (
    <div ref={box} className="nx-map" style={{ overflowY: 'auto', height: '100%' }}>
      <svg style={{ position: 'relative', width: '100%', height: h }} onClick={() => setSel(null)}>
        <defs>
          <marker id="arr" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L8,4 L0,8 z" fill="#e4e4e7" /></marker>
        </defs>

        {ticks.map((t, i) => <g key={i}>
          <line x1={x(t)} x2={x(t)} y1={TOP - 14} y2={h - 40} stroke="#fff" strokeOpacity=".05" />
          <text x={x(t)} y={TOP - 22} textAnchor="middle" fontSize="11" fill="#8b8b95">{day(t).slice(5)}</text>
        </g>)}

        {m.lanes.map((l) => {
          const c = STATE_HEX[l.state] ?? '#52525b', on = !hover || hover === l.id || m.rel.some((e) => (e.a === hover && e.b === l.id) || (e.b === hover && e.a === l.id))
          const x0 = x(l.first), x1 = Math.max(x(l.last), x0 + 12)
          return (
            <g key={l.id} opacity={on ? 1 : 0.2} style={{ transition: 'opacity .2s', cursor: 'pointer' }}
              onMouseEnter={() => setHover(l.id)} onMouseLeave={() => setHover(null)} onClick={(ev) => { ev.stopPropagation(); nav(`/thread/${encodeURIComponent(l.label)}`) }}>
              <text x={GUTTER - 14} y={y(l.id)} textAnchor="end" dy="0.35em" fontSize="12.5" fill={m.linked.has(l.id) ? '#f4f4f5' : '#8b8b95'}>{l.label}</text>
              <rect x={x0} y={y(l.id) - 7} width={x1 - x0} height="14" rx="7" fill={c} fillOpacity=".22" stroke={c} strokeOpacity=".8" />
              {l.q > 0 && <text x={x1 + 8} y={y(l.id)} dy="0.35em" fontSize="11" fill="#fbbf24">{l.q} asked</text>}
            </g>
          )
        })}

        {m.rel.map((e, i) => {
          const from = origin(e), to = target(e), ex = x(e.ts ?? m.lanes.find((l) => l.id === to)?.first)
          const y0 = y(from), y1 = y(to), dir = y1 > y0 ? 1 : -1, bow = 16 + (i % 3) * 6
          const proposed = e.state === 'proposed', on = sel === e
          return (
            <g key={i} opacity={lit(e) ? 1 : 0.1} style={{ cursor: 'pointer', transition: 'opacity .2s' }} onClick={(ev) => { ev.stopPropagation(); setSel(e) }}>
              <path d={`M${ex},${y0} C${ex + bow},${y0 + dir * 14} ${ex + bow},${y1 - dir * 14} ${ex},${y1 - dir * 8}`} fill="none" stroke="transparent" strokeWidth="14" />
              <path d={`M${ex},${y0} C${ex + bow},${y0 + dir * 14} ${ex + bow},${y1 - dir * 14} ${ex},${y1 - dir * 8}`} fill="none" markerEnd="url(#arr)"
                stroke={on ? '#60a5fa' : '#e4e4e7'} strokeOpacity={proposed ? 0.55 : 0.95} strokeWidth={on ? 2.2 : 1.5} strokeDasharray={proposed ? '4 3' : e.kind === 'built on' ? '1 0' : undefined} />
              <circle cx={ex} cy={y0} r="3.5" fill={on ? '#60a5fa' : '#e4e4e7'} />
            </g>
          )
        })}
      </svg>

      {sel && (
        <div className="absolute right-4 top-4 w-[360px] rounded-2xl border border-line2 bg-[#141416]/95 p-4 text-[13px] backdrop-blur" onClick={(e) => e.stopPropagation()}>
          <div className="text-[11.5px] uppercase tracking-wide text-mut">{day(sel.ts)} · {sel.state === 'proposed' ? 'proposed by Claude' : sel.state ?? 'filed'}</div>
          <div className="mt-1 text-[14.5px] font-semibold text-[#f4f4f5]">{name(sel.a)} <span className="font-normal text-soft">{sel.kind}</span> {name(sel.b)}</div>
          {sel.quote ? <blockquote className="mt-3 border-l-2 border-line2 pl-3 text-soft">“{sel.quote}”</blockquote> : <div className="mt-3 text-mut">Filed by hand, with no session quote.</div>}
          <div className="mt-3 flex items-center gap-3">
            {sel.source_id && <Link to={`/source/${sel.source_id}`} className="text-[#60a5fa] hover:underline">Open the session</Link>}
            {sel.state === 'proposed' && <>
              <button onClick={() => review(true)} className="ml-auto rounded-full bg-[#34d399] px-3 py-1 font-semibold text-black">Accept</button>
              <button onClick={() => review(false)} className="rounded-full border border-line2 px-3 py-1 text-soft">Reject</button>
            </>}
          </div>
        </div>
      )}

      <div className="mx-4 mb-3 flex w-fit flex-wrap gap-4 rounded-full border border-line2 bg-[#141416]/90 px-4 py-2 text-[12.5px] text-soft backdrop-blur">
        {Object.keys(STATE_NAME).map((k) => <span key={k} className="flex items-center gap-2"><span className="size-2.5 rounded-full" style={{ background: STATE_HEX[k] }} />{STATE_NAME[k]}</span>)}
      </div>
      <div className="mx-4 mb-6 max-w-[760px] text-[12px] text-mut">Each bar spans a project's first to last session. An arrow marks the day one project came out of, or was built on, another. Dashed arrows are proposed and carry the quote that supports them.</div>
    </div>
  )
}
