import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { dayFmt, useApi } from '@/lib/api'
import { STATE_HEX, STATE_NAME } from '@/lib/next'
import '@/next.css'

/** Every thread on one time axis: when it was worked on, what came out of it, how it ended, and what grew out of what. */

const DAY = 864e5
const ts = (d: string) => Date.UTC(+d.slice(0, 4), +d.slice(5, 7) - 1, +d.slice(8, 10))
const iso = (t: number) => new Date(t).toISOString().slice(0, 10)
const ORDER = ['built', 'moving', 'parked', 'dropped'] as const

/** Lanes for every thread with a session; with `days`, only the last `days` days and the threads active in them. */
function useTrail(days?: number) {
  const { data } = useApi('/api/trail') as { data: any }
  return useMemo(() => {
    if (!data) return null
    const per = new Map<string, any>()
    for (const t of data.threads) per.set(t.name, { ...t, s: [] as any[], made: [] as any[], i: [] as any[] })
    for (const s of data.sources) { const p = per.get(s.thread); if (p && s.d > '2000') (s.kind === 'session' ? p.s : p.made).push(s) }
    for (const i of data.items) per.get(i.thread)?.i.push(i)
    const now = new Date(), today = iso(Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()))
    const from = days ? iso(ts(today) - (days - 1) * DAY) : ''
    if (from) for (const p of per.values()) { p.s = p.s.filter((x: any) => x.d >= from); p.made = p.made.filter((x: any) => x.d >= from); p.i = p.i.filter((x: any) => x.d >= from) }
    const ps = [...per.values()].filter((p) => p.s.length)
    if (!ps.length) return { ps, links: [], d0: from || today, days: days ?? 1 }
    const all = ps.flatMap((p) => [...p.s, ...p.made, ...p.i].map((s: any) => s.d)).sort()
    const d0 = from || all[0], d1 = iso(Math.max(ts(all[all.length - 1]), ts(today)))
    for (const p of ps) {
      for (const x of [...p.made, ...p.i]) if (x.d < d0) x.d = d0
      const ds = [...p.s, ...p.made].map((x: any) => x.d).sort()
      p.first = ds[0]; p.last = ds[ds.length - 1]
    }
    ps.sort((a, b) => a.first.localeCompare(b.first))
    const names = new Set(ps.map((p) => p.name))
    const links: any[] = []
    for (const l of data.links) {
      const mm = l.text.match(/^(.+?) (built on|spun out of|forked from) (.+)$/)
      if (mm && names.has(mm[1]) && names.has(mm[3]) && !links.some((x) => x.from === mm[3] && x.to === mm[1])) links.push({ from: mm[3], to: mm[1], kind: mm[2], d: l.d })
    }
    return { ps, links: days ? [] : links, d0, days: (ts(d1) - ts(d0)) / DAY + 1 }
  }, [data, days])
}

/** The lane chart. Compact: shorter rows, no lineage arrows, for the Today page. */
export function TrailLanes({ m, compact = false }: { m: NonNullable<ReturnType<typeof useTrail>>; compact?: boolean }) {
  const nav = useNavigate()
  const [hover, setHover] = useState<string | null>(null)
  const [tip, setTip] = useState<{ x: number; y: number; h: ReactNode } | null>(null)
  const box = useRef<HTMLDivElement>(null)
  const [cw, setCw] = useState(0)
  useLayoutEffect(() => { if (!box.current) return; const ro = new ResizeObserver(([e]) => setCw(e.contentRect.width)); ro.observe(box.current); return () => ro.disconnect() }, [m])
  const { ps, links, d0, days } = m
  const LW = compact ? 180 : 210, RW = 128, RH = compact ? 46 : 46, TOP = 30, W = Math.max(860, cw || 1180), PW = W - LW - RW, dw = PW / days, H = TOP + ps.length * RH + 8
  const x = (d: string) => LW + ((ts(d) - ts(d0)) / DAY + 0.5) * dw
  const idx = new Map(ps.map((p, i) => [p.name, i]))
  const y = (i: number) => TOP + i * RH + RH / 2
  const show = (e: React.MouseEvent, h: ReactNode) => setTip({ x: e.clientX, y: e.clientY, h })
  const tick = compact ? 1 : 7
  return (
    <div ref={box} className="overflow-x-auto" onMouseLeave={() => setTip(null)}>
          <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`}>
            <defs><marker id="tah" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="6" markerHeight="6" orient="auto"><path d="M0 0L8 4L0 8z" fill="#6ee7b7" /></marker></defs>
            {Array.from({ length: Math.ceil(days / tick) }, (_, k) => iso(ts(d0) + k * tick * DAY)).map((d) => <g key={d}><line x1={x(d) - dw / 2} x2={x(d) - dw / 2} y1={TOP - 8} y2={H} stroke="#222226" /><text x={x(d) - dw / 2 + 4} y={TOP - 14} fill="#6b6b75" fontSize={11}>{dayFmt(d)}</text></g>)}
            {ps.map((p, i) => {
              const c = STATE_HEX[p.state] ?? '#8b8b95', yy = y(i), x0 = x(p.first) - 7, x1 = x(p.last) + 7
              const byDay: Record<string, any[]> = {}; p.s.forEach((s: any) => (byDay[s.d] ??= []).push(s))
              const madeDay: Record<string, any[]> = {}; p.made.forEach((s: any) => (madeDay[s.d] ??= []).push(s))
              const doneDay: Record<string, any[]> = {}; p.i.forEach((s: any) => (doneDay[s.d] ??= []).push(s))
              const dim = hover && hover !== p.name && !links.some((l) => (l.from === hover && l.to === p.name) || (l.to === hover && l.from === p.name))
              return (
                <g key={p.name} className="cursor-pointer" opacity={dim ? 0.45 : 1} onClick={() => nav(`/thread/${encodeURIComponent(p.name)}`)} onMouseEnter={() => setHover(p.name)} onMouseLeave={() => setHover(null)}>
                  <rect x={0} y={yy - RH / 2} width={W} height={RH} rx={8} fill={hover === p.name ? 'rgba(255,255,255,.035)' : 'transparent'} />
                  <circle cx={10} cy={yy} r={4} fill={c} />
                  <text x={22} y={yy + 4} fill="#e4e4e7" fontSize={13} fontWeight={500}>{p.name.length > 25 ? p.name.slice(0, 24) + '…' : p.name}<title>{p.summary}</title></text>
                  <rect x={x0} y={yy - 7} width={x1 - x0} height={14} rx={7} fill={c} fillOpacity={0.12} stroke={c} strokeOpacity={0.35} />
                  {Object.entries(byDay).map(([d, v]) => <circle key={d} cx={x(d)} cy={yy} r={Math.min(6.5, 2.6 + v.length * 0.9)} fill={c} onMouseMove={(e) => show(e, <><div className="font-semibold">{p.name}, {dayFmt(d)}</div><ul className="mt-1 list-disc pl-4 text-mut">{v.map((s) => <li key={s.id}>{s.title}</li>)}{(madeDay[d] ?? []).map((s) => <li key={s.id} className="text-[#c084fc]">Made: {s.title}</li>)}{(doneDay[d] ?? []).map((s, k) => <li key={k} className="text-[#34d399]">✓ {s.title}</li>)}</ul></>)} />)}
                  {Object.entries(madeDay).map(([d, v]) => { const xm = x(d), ym = yy - 15; return <rect key={d} x={xm - 4} y={ym - 4} width={8} height={8} transform={`rotate(45 ${xm} ${ym})`} fill="#c084fc" onMouseMove={(e) => show(e, <><div className="font-semibold">Made, {dayFmt(d)}</div><ul className="mt-1 list-disc pl-4 text-mut">{v.map((s) => <li key={s.id}>{s.title}</li>)}</ul></>)} /> })}
                  {Object.entries(doneDay).map(([d, v]) => <text key={d} x={x(d)} y={yy + 21} textAnchor="middle" fill="#34d399" fontSize={10} fontWeight={700} onMouseMove={(e) => show(e, <><div className="font-semibold">Done or decided, {dayFmt(d)}</div><ul className="mt-1 list-disc pl-4 text-mut">{v.map((s, k) => <li key={k}>{s.title}</li>)}</ul></>)}>✓{v.length > 1 ? v.length : ''}</text>)}
                  <text x={W - RW + 10} y={yy + 4} fill={c} fontSize={12} fontWeight={600}>{STATE_NAME[p.state] ?? p.state}</text>
                </g>)
            })}
            {links.map((l) => { const a = idx.get(l.from)!, b = idx.get(l.to)!, xx = x(l.d < d0 ? d0 : l.d), y0 = y(a), y1 = y(b), bend = 26 + Math.abs(b - a) * 2, on = hover === l.from || hover === l.to
              return <path key={l.from + l.to} d={`M${xx} ${y0} C ${xx + bend} ${y0}, ${xx + bend} ${y1}, ${xx} ${y1 + (y1 > y0 ? -6 : 6)}`} fill="none" stroke="#6ee7b7" strokeOpacity={on ? 0.95 : 0.2} strokeWidth={on ? 1.8 : 1.2} markerEnd="url(#tah)" pointerEvents="none" /> })}
          </svg>
      {tip && <div className="pointer-events-none fixed z-50 max-w-[340px] rounded-xl border border-line bg-[#1a1a1e] px-3 py-2 text-[12.5px] shadow-xl" style={{ left: Math.min(tip.x + 14, window.innerWidth - 350), top: tip.y + 14 }}>{tip.h}</div>}
    </div>
  )
}

export function TrailCard() {
  const full = useTrail(14)
  const m = useMemo(() => full && { ...full, ps: [...full.ps].sort((a, b) => b.last.localeCompare(a.last)).slice(0, 8) }, [full])
  if (!full || !m || !m.ps.length) return null
  const more = full.ps.length - m.ps.length
  return (
    <section className="nx-panel nx-in mt-5 px-5 pb-3 pt-4" style={{ ['--d' as any]: '280ms' }}>
      <div className="mb-1 flex items-baseline justify-between gap-3">
        <h3 className="text-[15px] font-semibold">Last two weeks <span className="ml-2 text-[12.5px] font-normal text-mut">{full.ps.length} threads worked on{more > 0 ? `, the ${m.ps.length} most recent shown` : ''}</span></h3>
        <Link to="/graph?view=trail" className="text-[12.5px] font-medium text-accent hover:underline">Full trail</Link>
      </div>
      <TrailLanes m={m} compact />
    </section>
  )
}

export default function Trail({ tabs }: { tabs: ReactNode }) {
  const full = useTrail()
  const [sort, setSort] = useState<'last' | 'first'>('last')
  const [only, setOnly] = useState<string | null>(null)
  const m = useMemo(() => {
    if (!full) return null
    const ps = full.ps.filter((p) => !only || p.state === only).sort((a, b) => sort === 'last' ? b.last.localeCompare(a.last) || b.first.localeCompare(a.first) : a.first.localeCompare(b.first))
    const names = new Set(ps.map((p) => p.name))
    return { ...full, ps, links: full.links.filter((l) => names.has(l.from) && names.has(l.to)) }
  }, [full, sort, only])
  if (!full || !m) return <div className="nx-page"><div className="text-mut">Loading</div></div>
  const { ps } = full
  const seg = (on: boolean, c = '#34d399') => ({ height: 26, border: '1px solid var(--color-line2)', background: on ? c + '26' : 'rgba(255,255,255,.04)', color: on ? c : 'var(--color-soft)' })
  const made = ps.reduce((a, p) => a + p.made.length, 0)
  return (
    <div className="nx-page">
      <div className="nx-in"><div className="text-[13px] text-mut">Every thread on one timeline, how each one ended, and what came out of it</div>
        <div className="flex items-center gap-4"><h2 className="text-[28px] font-semibold leading-tight tracking-tight">Graph</h2>{tabs}</div></div>

      <div className="mt-5 grid grid-cols-5 gap-3 max-[1100px]:grid-cols-3">
        {ORDER.map((st, k) => { const g = ps.filter((p) => p.state === st); return (
          <button key={st} type="button" title={only === st ? 'Show all threads' : `Show only ${STATE_NAME[st]}`} onClick={() => setOnly(only === st ? null : st)} className="nx-tile nx-in text-left" style={{ ['--d' as any]: `${40 + k * 40}ms`, opacity: only && only !== st ? 0.5 : 1, outline: only === st ? `1px solid ${STATE_HEX[st]}` : undefined }}>
            <div className="nx-big" style={{ color: STATE_HEX[st] }}>{g.length}</div><div className="mt-1 text-[13px] text-soft">{STATE_NAME[st]}</div>
            <div className="mt-1.5 text-[12px] leading-snug text-mut">{g.map((p) => p.name).join(', ')}</div>
          </button>) })}
        <div className="nx-tile nx-in" style={{ ['--d' as any]: '200ms' }}><div className="nx-big text-[#c084fc]">{made}</div><div className="mt-1 text-[13px] text-soft">Things made</div><div className="mt-1.5 text-[12px] text-mut">artifacts and commit runs</div></div>
      </div>

      <section className="nx-panel nx-in mt-5 px-5 pb-3 pt-4" style={{ ['--d' as any]: '240ms' }}>
        <div className="mb-2 flex flex-wrap items-baseline justify-between gap-3">
          <div className="flex flex-wrap items-center gap-3">
            <h3 className="text-[15px] font-semibold">Trail <span className="ml-2 text-[12.5px] font-normal text-mut">{only ? `${m.ps.length} ${STATE_NAME[only].toLowerCase()}, ` : ''}click a thread to open its story</span></h3>
            <div className="flex gap-1">{([['last', 'Latest work'], ['first', 'First started']] as const).map(([k, l]) => <button key={k} type="button" onClick={() => setSort(k)} className="rounded-full px-2.5 text-[12px] font-medium" style={seg(sort === k)}>{l}</button>)}</div>
            {only && <button type="button" onClick={() => setOnly(null)} className="rounded-full px-2.5 text-[12px] font-medium" style={seg(true, STATE_HEX[only])}>{STATE_NAME[only]} ×</button>}
          </div>
          <div className="flex flex-wrap gap-4 text-[12px] text-mut">
            {ORDER.map((st) => <span key={st} className="flex items-center gap-1.5"><span className="size-2 rounded-full" style={{ background: STATE_HEX[st] }} />{STATE_NAME[st]}</span>)}
            <span className="flex items-center gap-1.5"><span className="size-2 rotate-45 bg-[#c084fc]" />Made</span>
            <span className="flex items-center gap-1.5"><span className="text-[#34d399]">✓</span>Done or decided</span>
            <span className="flex items-center gap-1.5"><span className="size-2 rounded-full border border-[#6ee7b7]" />Grew out of</span>
          </div>
        </div>
        <TrailLanes m={m} />
      </section>
    </div>
  )
}
