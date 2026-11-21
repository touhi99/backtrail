import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { ArrowLeft, Lightbulb, X } from 'lucide-react'
import { dayFmt, useApi } from '@/lib/api'
import { STATE_HEX, STATE_NAME } from '@/lib/next'
import type { DChain, DDecision, DEdge, DLink, DOverview, DProject, DProjectView } from '@/lib/types'
import '@/next.css'

const AMBER = '#fbbf24', GREEN = '#34d399', VIOLET = '#c084fc', BLUE = '#60a5fa', MUT = '#8b8b95'

function useWidth() {
  const ref = useRef<HTMLDivElement>(null)
  const [w, setW] = useState(900)
  useEffect(() => {
    const el = ref.current; if (!el) return
    const ro = new ResizeObserver(() => setW(el.clientWidth))
    ro.observe(el); setW(el.clientWidth)
    return () => ro.disconnect()
  }, [])
  return { ref, w }
}

const srcLink = (d: { source_id: number | null; chunk_id: number | null }) => d.source_id ? `/source/${d.source_id}${d.chunk_id ? `?c=${d.chunk_id}` : ''}` : '#'

function groups(decisions: DDecision[], links: DLink[]) {
  const parent = new Map<string, string>(decisions.map((d) => [d.id, d.id]))
  const find = (x: string): string => { const p = parent.get(x) ?? x; if (p === x) return x; const r = find(p); parent.set(x, r); return r }
  for (const l of links) if (l.kind === 'restates' && parent.has(l.a) && parent.has(l.b)) parent.set(find(l.a), find(l.b))
  const by = new Map<string, DDecision[]>()
  for (const d of decisions) { const r = find(d.id); by.set(r, [...(by.get(r) ?? []), d]) }
  const head = new Map<string, DDecision>()
  for (const ms of by.values()) {
    const sorted = ms.slice().sort((a, b) => a.day.localeCompare(b.day))
    const top = sorted.find((m) => m.state === 'settled' && !m.superseded_by) ?? sorted[sorted.length - 1]
    for (const m of ms) head.set(m.id, top)
  }
  return { head, members: (id: string) => by.get(find(id)) ?? [] }
}

const STATE_C: Record<string, string> = { moving: STATE_HEX.moving, open: STATE_HEX.open, built: STATE_HEX.built, parked: STATE_HEX.parked }
const radius = (p: DProject) => 10 + Math.sqrt(p.n) * 2.6

function layoutProjects(ps: DProject[], edges: DEdge[], W: number, H: number) {
  const pos = new Map<string, { x: number; y: number }>()
  const linked = new Set(edges.flatMap((e) => [e.a, e.b]))
  const core = ps.filter((p) => linked.has(p.name)), loose = ps.filter((p) => !linked.has(p.name))
  const GA = Math.PI * (3 - Math.sqrt(5))
  core.forEach((p, i) => { const r = 40 + Math.sqrt(i + 1) * 46; pos.set(p.name, { x: W / 2 + Math.cos(i * GA) * r * 1.5, y: H / 2 + Math.sin(i * GA) * r }) })
  loose.forEach((p, i) => { const a = (i / Math.max(loose.length, 1)) * Math.PI * 2 + 0.3; pos.set(p.name, { x: W / 2 + Math.cos(a) * W * 0.42, y: H / 2 + Math.sin(a) * H * 0.4 }) })
  const rad = new Map(ps.map((p) => [p.name, radius(p)]))
  for (let it = 0; it < 500; it++) {
    for (let i = 0; i < ps.length; i++) for (let j = i + 1; j < ps.length; j++) {
      const a = pos.get(ps[i].name)!, b = pos.get(ps[j].name)!
      const dx = a.x - b.x, dy = a.y - b.y, d = Math.hypot(dx, dy) || 1
      const want = rad.get(ps[i].name)! + rad.get(ps[j].name)! + 90
      if (d < want) { const k = (want - d) / d * 0.5; a.x += dx * k; a.y += dy * k; b.x -= dx * k; b.y -= dy * k }
    }
    for (const e of edges) {
      const a = pos.get(e.a), b = pos.get(e.b); if (!a || !b) continue
      const dx = b.x - a.x, dy = b.y - a.y, d = Math.hypot(dx, dy) || 1, k = (d - 190) / d * 0.02
      a.x += dx * k; a.y += dy * k; b.x -= dx * k; b.y -= dy * k
    }
    for (const p of ps) {
      const q = pos.get(p.name)!, r = rad.get(p.name)!
      q.x += (W / 2 - q.x) * 0.004; q.y += (H / 2 - q.y) * 0.006
      q.x = Math.min(W - r - 70, Math.max(r + 70, q.x)); q.y = Math.min(H - r - 40, Math.max(r + 30, q.y))
    }
  }
  return pos
}

function Overview({ data, open }: { data: DOverview; open: (p: string) => void }) {
  const [hover, setHover] = useState('')
  const { ref, w: W } = useWidth()
  const H = 660
  const edges = useMemo(() => data.edges.filter((e) => e.kind !== 'shared'), [data])
  const pos = useMemo(() => layoutProjects(data.projects, edges, W, H), [data, edges, W])
  const near = (n: string) => !hover || n === hover || edges.some((e) => (e.a === hover && e.b === n) || (e.b === hover && e.a === n))
  const cur = data.projects.find((p) => p.name === hover)
  return (
    <div ref={ref} className="nx-strands relative w-full overflow-hidden" style={{ height: H }}>
      <svg width={W} height={H} className="absolute inset-0">
        <defs>
          <filter id="oglow" x="-100%" y="-100%" width="300%" height="300%"><feGaussianBlur stdDeviation="9" result="b" /><feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
          {Object.entries(STATE_C).map(([k, c]) => <radialGradient key={k} id={`og-${k}`} cx="35%" cy="30%"><stop offset="0%" stopColor="#fff" stopOpacity=".55" /><stop offset="45%" stopColor={c} /><stop offset="100%" stopColor={c} stopOpacity=".55" /></radialGradient>)}
          <marker id="oamb" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill={AMBER} /></marker>
        </defs>
        {STARS.map((st, i) => <circle key={i} cx={st.x * W} cy={st.y * H} r={st.r} fill="#fff" opacity={0.12} />)}
        {edges.map((e, i) => {
          const a = pos.get(e.a), b = pos.get(e.b); if (!a || !b) return null
          const pa = data.projects.find((p) => p.name === e.a)!, pb = data.projects.find((p) => p.name === e.b)!
          const dx = b.x - a.x, dy = b.y - a.y, d = Math.hypot(dx, dy) || 1
          const sx = a.x + (dx / d) * (radius(pa) + 4), sy = a.y + (dy / d) * (radius(pa) + 4), ex = b.x - (dx / d) * (radius(pb) + 6), ey = b.y - (dy / d) * (radius(pb) + 6)
          const mx = (sx + ex) / 2 - dy * 0.15, my = (sy + ey) / 2 + dx * 0.15
          const lit = !hover || e.a === hover || e.b === hover
          return e.kind === 'changed'
            ? <path key={i} d={`M${sx},${sy} Q${mx},${my} ${ex},${ey}`} fill="none" stroke={AMBER} strokeWidth={lit && hover ? 2.4 : 1.6} opacity={lit ? 0.9 : 0.06} markerEnd="url(#oamb)" filter={lit && hover ? 'url(#glow)' : undefined}><title>{`${e.was ?? ''} → ${e.label}`}</title></path>
            : <path key={i} d={`M${sx},${sy} Q${mx},${my} ${ex},${ey}`} fill="none" stroke={BLUE} strokeWidth={1.2} strokeDasharray="4 5" opacity={lit ? 0.5 : 0.05}><title>{e.label}</title></path>
        })}
        {data.projects.map((p) => {
          const q = pos.get(p.name); if (!q) return null
          const r = radius(p), lit = near(p.name), key = STATE_C[p.state] ? p.state : 'parked'
          return (
            <g key={p.name} style={{ cursor: 'pointer' }} opacity={lit ? 1 : 0.18} onMouseEnter={() => setHover(p.name)} onMouseLeave={() => setHover('')} onClick={() => open(p.name)}>
              {p.state === 'moving' && <circle className="nx-pulse" cx={q.x} cy={q.y} r={r + 6} fill="none" stroke={STATE_HEX.moving} strokeOpacity={0.4} />}
              <circle cx={q.x} cy={q.y} r={r} fill={`url(#og-${key})`} filter="url(#oglow)" opacity={0.95} />
              {p.turns > 0 && <circle cx={q.x} cy={q.y} r={r + 3} fill="none" stroke={AMBER} strokeOpacity={0.7} strokeWidth={1.5} strokeDasharray={`${Math.min(p.turns, 12) * 4} 999`} />}
              <text x={q.x} y={q.y + r + 17} textAnchor="middle" fontSize={13} fontWeight={600} fill="#e4e4e7">{p.name}</text>
            </g>
          )
        })}
      </svg>
      {cur && (() => { const q = pos.get(cur.name)!; const r = radius(cur); return (
        <div className="pointer-events-none absolute z-10 w-[270px] rounded-2xl border border-[#2b2b33] bg-[#0f0f12]/95 px-4 py-3 shadow-2xl backdrop-blur" style={{ left: Math.min(Math.max(q.x - 135, 12), W - 282), top: q.y + r + 28 > H - 120 ? q.y - r - 110 : q.y + r + 28 }}>
          <div className="text-[11.5px] text-mut">{dayFmt(cur.headline.day)}</div>
          <div className="line-clamp-3 text-[14px] leading-snug">{cur.headline.title}</div>
        </div>) })()}
    </div>
  )
}

function laneOf(d: DDecision, project: string, rank: Map<string, number>) {
  const others = d.subjects.filter((s) => s !== project)
  if (!others.length) return ''
  return others.slice().sort((a, b) => (rank.get(b) ?? 0) - (rank.get(a) ?? 0))[0]
}

const live = (d: DDecision) => d.state === 'settled' && !d.superseded_by

function chainOf(start: string, links: DLink[]) {
  const seen = new Set<string>()
  const stack = [start]
  while (stack.length) {
    const cur = stack.pop()!
    if (seen.has(cur)) continue
    seen.add(cur)
    for (const l of links) { if (l.a === cur) stack.push(l.b); if (l.b === cur) stack.push(l.a) }
  }
  return seen
}

const STARS = Array.from({ length: 70 }, (_, i) => ({ x: (i * 977) % 1000 / 1000, y: (i * 613) % 1000 / 1000, r: (i % 3) * 0.35 + 0.4 }))

function Strands({ view, focus, pick }: { view: DProjectView; focus: string; pick: (id: string) => void }) {
  const { ref, w } = useWidth()
  const [tip, setTip] = useState<{ d: DDecision; x: number; y: number } | null>(null)
  const model = useMemo(() => {
    const { head } = groups(view.decisions, view.links)
    const heads = [...new Set(view.decisions.map((d) => head.get(d.id)!))]
    const arcs = view.links.filter((l) => l.kind === 'changed').map((l) => ({ from: head.get(l.b), to: head.get(l.a) })).filter((a): a is { from: DDecision; to: DDecision } => !!a.from && !!a.to && a.from.id !== a.to.id)
    const chained = new Set(arcs.flatMap((a) => [a.from.id, a.to.id]))
    const rank = new Map<string, number>()
    for (const d of view.decisions) for (const x of d.subjects) if (x !== view.name) rank.set(x, (rank.get(x) ?? 0) + 1)
    const subj = new Map(heads.map((h) => [h.id, laneOf(h, view.name, rank) || view.name]))
    const score = new Map<string, { c: number; n: number }>()
    for (const h of heads) { const k = subj.get(h.id)!; if (!k) continue; const v = score.get(k) ?? { c: 0, n: 0 }; v.n++; if (chained.has(h.id)) v.c++; score.set(k, v) }
    const lit = focus ? chainOf(focus, view.links) : new Set<string>()
    const must = [...new Set(heads.filter((h) => lit.has(h.id) || view.decisions.some((m) => head.get(m.id)?.id === h.id && lit.has(m.id))).map((h) => subj.get(h.id)!))]
    const ranked = [...score.entries()].sort((a, b) => b[1].c - a[1].c || b[1].n - a[1].n).map(([k]) => k).filter((k) => !must.includes(k))
    const strands = [...must, ...ranked].slice(0, Math.max(6, must.length))
    const nodes: { d: DDecision; strand: string; now: boolean }[] = []
    for (const st of strands) {
      const mine = heads.filter((h) => subj.get(h.id) === st && !h.outside)
      const current = mine.filter(live).sort((a, b) => b.day.localeCompare(a.day))[0]
      for (const h of heads.filter((h) => subj.get(h.id) === st && chained.has(h.id))) nodes.push({ d: h, strand: st, now: h.id === current?.id })
      if (current && !chained.has(current.id)) nodes.push({ d: current, strand: st, now: true })
    }
    const shown = new Set(nodes.map((n) => n.d.id))
    return { head, strands, nodes, arcs: arcs.filter((a) => shown.has(a.from.id) && shown.has(a.to.id)) }
  }, [view, focus])
  const H = 120 + model.strands.length * 84
  const L = 150, R = 230
  const days = model.nodes.map((n) => Date.parse(n.d.day))
  const t0 = Math.min(...days), t1 = Math.max(...days)
  const x = (day: string) => L + (t1 === t0 ? 0.5 : (Date.parse(day) - t0) / (t1 - t0)) * Math.max(120, w - L - R)
  const y = (st: string) => 56 + model.strands.indexOf(st) * 84
  const px = new Map<string, number>()
  for (const st of model.strands) {
    let last = -Infinity
    for (const n of model.nodes.filter((m) => m.strand === st).sort((a, b) => x(a.d.day) - x(b.d.day))) { const v = Math.max(x(n.d.day), last + 20); px.set(n.d.id, v); last = v }
  }
  const nx = (d: DDecision) => px.get(d.id) ?? x(d.day)
  const lit = focus ? chainOf(focus, view.links) : null
  const on = (id: string) => !lit || lit.has(id) || view.decisions.some((m) => model.head.get(m.id)?.id === id && lit.has(m.id))
  const ticks = t1 > t0 ? [0, 0.25, 0.5, 0.75, 1].map((f) => new Date(t0 + f * (t1 - t0)).toISOString().slice(0, 10)) : []
  if (!model.strands.length) return <div ref={ref} className="px-6 py-16 text-center text-mut">No decisions with a subject yet.</div>
  return (
    <div ref={ref} className="nx-strands relative w-full overflow-hidden" style={{ height: H }}>
      <svg width={w} height={H} className="absolute inset-0">
        <defs>
          <filter id="glow" x="-80%" y="-80%" width="260%" height="260%"><feGaussianBlur stdDeviation="5" result="b" /><feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
          <linearGradient id="strand" x1="0" x2="1"><stop offset="0" stopColor="#3a3a44" stopOpacity="0" /><stop offset=".15" stopColor="#3a3a44" /><stop offset="1" stopColor="#4a4a55" /></linearGradient>
          <linearGradient id="trunk" x1="0" x2="1"><stop offset="0" stopColor={GREEN} stopOpacity="0" /><stop offset=".3" stopColor={GREEN} stopOpacity=".5" /><stop offset="1" stopColor={GREEN} /></linearGradient>
        </defs>
        {STARS.map((st, i) => <circle key={i} cx={st.x * w} cy={st.y * H} r={st.r} fill="#fff" opacity={0.12} />)}
        {model.strands.map((st) => {
          const xs = model.nodes.filter((n) => n.strand === st).map((n) => nx(n.d))
          return <path key={st} d={`M${Math.min(...xs) - 60},${y(st)} L${Math.max(...xs)},${y(st)}`} stroke="url(#strand)" strokeWidth={2} opacity={lit ? 0.35 : 1} />
        })}
        <line x1={L - 20} x2={w - R + 40} y1={H - 44} y2={H - 44} stroke="url(#trunk)" strokeWidth={2} filter="url(#glow)" />
        {ticks.map((d) => <text key={d} x={x(d)} y={H - 20} fontSize={11.5} fill={MUT} textAnchor="middle">{dayFmt(d)}</text>)}
        {model.arcs.map((a, i) => {
          const fx = nx(a.from), fy = y(model.nodes.find((n) => n.d.id === a.from.id)!.strand), tx = nx(a.to), ty = y(model.nodes.find((n) => n.d.id === a.to.id)!.strand)
          const bright = lit ? on(a.from.id) && on(a.to.id) : true
          const lift = fy === ty ? -Math.max(26, Math.abs(tx - fx) * 0.22) : 0
          return <path key={i} d={`M${fx},${fy} C${fx + (tx - fx) * 0.4},${fy + lift} ${tx - (tx - fx) * 0.4},${ty + lift} ${tx},${ty}`} fill="none" stroke={AMBER} strokeWidth={bright && lit ? 2.6 : 1.8} opacity={bright ? (lit ? 1 : 0.7) : 0.06} filter={bright && lit ? 'url(#glow)' : undefined} />
        })}
        {model.nodes.map(({ d, strand, now }) => {
          const cx = nx(d), cy = y(strand), alive = live(d), sel = focus && on(d.id)
          return (
            <g key={d.id} style={{ cursor: 'pointer' }} opacity={on(d.id) ? 1 : 0.15} onClick={() => pick(d.id)} onMouseEnter={() => setTip({ d, x: cx, y: cy })} onMouseLeave={() => setTip(null)}>
              <circle cx={cx} cy={cy} r={16} fill="transparent" />
              {now && <circle className="nx-pulse" cx={cx} cy={cy} r={15} fill="none" stroke={GREEN} strokeOpacity={0.45} />}
              {alive ? <circle cx={cx} cy={cy} r={now ? 8 : 6} fill={GREEN} filter="url(#glow)" /> : <circle cx={cx} cy={cy} r={5.5} fill="#0e0e11" stroke="#6b6b75" strokeWidth={1.6} />}
              {sel && <circle cx={cx} cy={cy} r={11} fill="none" stroke="#fff" strokeOpacity={0.7} />}
            </g>
          )
        })}
      </svg>
      {model.strands.map((st) => <div key={st} className="absolute left-5 w-[118px] truncate text-[13px] font-semibold text-soft" style={{ top: y(st) - 10, opacity: lit ? 0.4 : 1 }} title={st}>{st}</div>)}
      {model.nodes.filter((n) => n.now).map(({ d, strand }) => (
        <button key={d.id} type="button" onClick={() => pick(d.id)} className="absolute line-clamp-2 w-[200px] text-left text-[13px] leading-snug text-ink transition hover:text-white" style={{ left: nx(d) + 18, top: y(strand) - 17, opacity: on(d.id) ? 1 : 0.15 }}>{d.title}</button>
      ))}
      {tip && !tip.d.outside && (
        <div className="pointer-events-none absolute z-10 w-[260px] rounded-xl border border-[#2b2b33] bg-[#0f0f12]/95 px-3 py-2 text-[12.5px] leading-snug shadow-xl backdrop-blur" style={{ left: Math.min(tip.x + 14, w - 270), top: tip.y + 14 }}>
          <div className="text-[11.5px] text-mut">{dayFmt(tip.d.day)}</div>{tip.d.title}
        </div>
      )}
    </div>
  )
}

function Chain({ id, project, close }: { id: string; project: string; close: () => void }) {
  const { data } = useApi<DChain>(`/api/kg/chain?id=${encodeURIComponent(id)}`)
  const steps = useMemo(() => {
    if (!data) return []
    const { head, members } = groups(data.decisions, data.links)
    const heads = [...new Set(data.decisions.map((d) => head.get(d.id)!))]
    const depth = new Map<string, number>(heads.map((h) => [h.id, 0]))
    const changed = data.links.filter((l) => l.kind === 'changed').map((l) => ({ from: head.get(l.b)?.id, to: head.get(l.a)?.id, reason: l.reason })).filter((l) => l.from && l.to && l.from !== l.to)
    for (let i = 0; i < heads.length; i++) for (const l of changed) depth.set(l.to!, Math.max(depth.get(l.to!) ?? 0, (depth.get(l.from!) ?? 0) + 1))
    const levels: { h: DDecision; also: DDecision[]; because: DDecision['because'] }[][] = []
    for (const h of heads.sort((a, b) => a.day.localeCompare(b.day))) {
      const d = depth.get(h.id) ?? 0
      while (levels.length <= d) levels.push([])
      levels[d].push({ h, also: members(h.id).filter((m) => m.id !== h.id), because: members(h.id).flatMap((m) => m.because) })
    }
    return levels.filter((l) => l.length)
  }, [data])
  return (
    <aside className="nx-in relative flex max-h-[calc(100vh-150px)] flex-col overflow-y-auto rounded-2xl border border-[#26262c] bg-[#111114]/95 p-5 shadow-2xl">
      <button type="button" aria-label="Close" onClick={close} className="absolute right-4 top-4 grid size-7 place-items-center rounded-full text-mut transition hover:bg-white/10 hover:text-ink"><X size={15} /></button>
      {!data && <div className="text-[13px] text-mut">Loading</div>}
      {data && steps.length === 0 && <div className="text-[13px] text-mut">This decision is not in the store any more.</div>}
      <ol className="relative flex flex-col">
        {steps.map((level, i) => (
          <li key={i} className="relative flex flex-col gap-4 pb-6 pl-7 last:pb-0">
            {i < steps.length - 1 && <span className="absolute bottom-0 left-[7px] top-3 w-[2px]" style={{ background: `linear-gradient(${AMBER}, ${AMBER}55)` }} />}
            {level.map(({ h, also, because }) => {
              const alive = live(h)
              return (
                <div key={h.id} className="relative flex flex-col gap-2">
                  <span className="absolute -left-7 top-1 grid size-4 place-items-center rounded-full" style={alive ? { background: GREEN, boxShadow: `0 0 12px ${GREEN}` } : { background: '#0e0e11', border: '1.6px solid #6b6b75' }} />
                  <span className="flex items-center gap-2 text-[12px]" style={{ color: alive ? GREEN : MUT }}>
                    {dayFmt(h.day)}{alive && <span className="rounded-full px-2 py-px text-[11px] font-medium" style={{ background: GREEN + '22' }}>Now</span>}
                    {h.thread !== project && <span className="rounded-full border border-[#2b2b33] px-2 py-px text-[11px] text-soft">{h.thread}</span>}
                  </span>
                  <Link to={srcLink(h)} className={`text-[17px] leading-snug transition hover:text-white ${alive ? 'text-ink' : 'text-soft'}`}>{h.quote ? `“${h.quote}”` : h.title}</Link>
                  {h.quote && <span className="text-[12.5px] leading-snug text-mut">{h.title}</span>}
                  {because.length > 0 && (
                    <div className="flex flex-wrap gap-1.5">
                      {because.map((b) => <Link key={b.id} to={srcLink(b)} title={b.title} className="inline-flex max-w-full items-center gap-1 truncate rounded-full px-2.5 py-1 text-[12px] transition hover:brightness-125" style={{ background: VIOLET + '1f', color: VIOLET }}><Lightbulb size={11} className="shrink-0" /><span className="truncate">{b.title}</span></Link>)}
                    </div>
                  )}
                  {also.length > 0 && <details className="text-[12px] text-mut"><summary className="cursor-pointer select-none hover:text-ink">Said {also.length + 1} times</summary><div className="mt-1.5 flex flex-col gap-1">{also.map((m) => <Link key={m.id} to={srcLink(m)} className="hover:text-ink">{dayFmt(m.day)}: “{m.quote || m.title}”</Link>)}</div></details>}
                </div>
              )
            })}
          </li>
        ))}
      </ol>
    </aside>
  )
}

export default function DecisionGraph({ tabs }: { tabs: React.ReactNode }) {
  const [sp, setSp] = useSearchParams()
  const project = sp.get('p') ?? ''
  const raw = sp.get('d')
  const set = (k: string, v: string) => { const n = new URLSearchParams(sp); if (v) n.set(k, v); else n.delete(k); setSp(n) }
  const { data: over } = useApi<DOverview>(project ? null : '/api/kg/decisions')
  const { data: view } = useApi<DProjectView>(project ? `/api/kg/decisions?project=${encodeURIComponent(project)}` : null)
  const newestTurn = useMemo(() => {
    if (!view) return ''
    const by = new Map(view.decisions.map((d) => [d.id, d]))
    return view.links.filter((l) => l.kind === 'changed' && by.get(l.a) && !by.get(l.a)!.outside).sort((a, b) => by.get(b.a)!.day.localeCompare(by.get(a.a)!.day))[0]?.a ?? ''
  }, [view])
  const focus = raw === '-' ? '' : raw ?? newestTurn
  return (
    <div className="nx-page">
      <div className="relative mx-auto flex max-w-[1240px] flex-col gap-5">
        <header className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <div className="flex items-center gap-4"><h2 className="text-[34px] font-semibold leading-tight tracking-tight">{project || 'Graph'}</h2>{tabs}</div>
          </div>
          {project && <button type="button" className="btn ghost" onClick={() => { const n = new URLSearchParams(sp); n.delete('p'); n.delete('d'); setSp(n) }}><ArrowLeft size={14} />All projects</button>}
        </header>
        {!project && (
          over ? (
            <section className="relative overflow-hidden rounded-2xl border border-[#1f1f25]" style={{ background: 'radial-gradient(900px 420px at 50% 40%, rgba(52,211,153,.07), transparent 70%), #0c0c0f' }}>
              <div className="absolute right-5 top-4 z-10 flex gap-4 text-[12px] text-mut">
                <span className="flex items-center gap-1.5"><span className="size-2.5 rounded-full" style={{ background: STATE_HEX.moving, boxShadow: `0 0 8px ${STATE_HEX.moving}` }} />{STATE_NAME.moving}</span>
                <span className="flex items-center gap-1.5"><span className="size-2.5 rounded-full" style={{ background: STATE_HEX.built }} />{STATE_NAME.built}</span>
                <span className="flex items-center gap-1.5"><span className="h-[2px] w-4 rounded" style={{ background: AMBER }} />Changed</span>
                <span className="flex items-center gap-1.5"><span className="h-0 w-4 border-t-2 border-dashed" style={{ borderColor: BLUE }} />Built on</span>
              </div>
              <Overview data={over} open={(p) => set('p', p)} />
            </section>
          ) : <div className="text-mut">Loading</div>
        )}
        {project && view && (
          <div className="flex items-start gap-4">
            <section className="relative min-w-0 flex-1 overflow-hidden rounded-2xl border border-[#1f1f25]" style={{ background: 'radial-gradient(900px 420px at 80% 0%, rgba(52,211,153,.08), transparent 70%), #0c0c0f' }}>
              <div className="absolute right-5 top-4 z-10 flex gap-4 text-[12px] text-mut">
                <span className="flex items-center gap-1.5"><span className="size-2.5 rounded-full" style={{ background: GREEN, boxShadow: `0 0 8px ${GREEN}` }} />Now</span>
                <span className="flex items-center gap-1.5"><span className="size-2.5 rounded-full border-[1.6px] border-[#6b6b75]" />Replaced</span>
                <span className="flex items-center gap-1.5"><span className="h-[2px] w-4 rounded" style={{ background: AMBER }} />Changed</span>
              </div>
              <Strands view={view} focus={focus} pick={(id) => set('d', id === focus ? '-' : id)} />
            </section>
            {focus && <div className="sticky top-4 w-[360px] shrink-0"><Chain id={focus} project={project} close={() => set('d', '-')} /></div>}
          </div>
        )}
        {project && !view && <div className="text-mut">Loading</div>}
      </div>
    </div>
  )
}
