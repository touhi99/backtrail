import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { Minus, Plus, Search as SearchIcon, X, Maximize2 } from 'lucide-react'
import { dayFmt, post, useApi } from '@/lib/api'
import '@/next.css'

const KIND_C: Record<string, string> = { project: '#60a5fa', service: '#c084fc', tool: '#fbbf24', site: '#34d399' }
const STATUS_C: Record<string, string> = { active: '#34d399', proposed: '#8b8b95', superseded: '#fbbf24', retracted: '#fb7185' }
const STATUS_L: Record<string, string> = { active: 'kept', proposed: 'suggested', superseded: 'replaced', retracted: 'rejected' }
const WHO: Record<string, string> = { owner: 'You', assistant: 'Claude', unknown: 'Unknown' }
const W = 880, H = 700

/** Fixed-seed force layout: repulsion between nodes, springs along shared-claim edges, a weak pull to the centre. */
function layout(nodes: any[], edges: any[]) {
  const pos = new Map<string, { x: number; y: number }>()
  nodes.forEach((n, i) => { const a = (i / nodes.length) * Math.PI * 2; pos.set(n.id, { x: W / 2 + Math.cos(a) * 300, y: H / 2 + Math.sin(a) * 250 }) })
  for (let it = 0; it < 360; it++) {
    const f = new Map<string, { x: number; y: number }>(nodes.map((n) => [n.id, { x: 0, y: 0 }]))
    for (let i = 0; i < nodes.length; i++) for (let j = i + 1; j < nodes.length; j++) {
      const a = pos.get(nodes[i].id)!, b = pos.get(nodes[j].id)!
      const dx = a.x - b.x, dy = a.y - b.y, d2 = Math.max(dx * dx + dy * dy, 60), k = 17000 / d2, d = Math.sqrt(d2)
      f.get(nodes[i].id)!.x += (dx / d) * k; f.get(nodes[i].id)!.y += (dy / d) * k
      f.get(nodes[j].id)!.x -= (dx / d) * k; f.get(nodes[j].id)!.y -= (dy / d) * k
    }
    for (const e of edges) {
      const a = pos.get(e.a), b = pos.get(e.b); if (!a || !b) continue
      const dx = b.x - a.x, dy = b.y - a.y, d = Math.sqrt(dx * dx + dy * dy) || 1, k = (d - 165) * 0.008 * Math.min(1 + Math.log(e.w), 3)
      f.get(e.a)!.x += (dx / d) * k; f.get(e.a)!.y += (dy / d) * k; f.get(e.b)!.x -= (dx / d) * k; f.get(e.b)!.y -= (dy / d) * k
    }
    for (const n of nodes) {
      const p = pos.get(n.id)!, g = f.get(n.id)!
      g.x += (W / 2 - p.x) * 0.0035; g.y += (H / 2 - p.y) * 0.007
      p.x = Math.min(W - 70, Math.max(70, p.x + Math.max(-10, Math.min(10, g.x)))); p.y = Math.min(H - 50, Math.max(40, p.y + Math.max(-10, Math.min(10, g.y))))
    }
  }
  return pos
}

function ClaimCard({ c, ev, showEntities }: { c: any; ev: any[]; showEntities?: boolean }) {
  return (
    <div className="rounded-[14px] border border-line bg-white/[.025] p-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="nx-chip" style={{ background: `${STATUS_C[c.status] ?? '#8b8b95'}22`, color: STATUS_C[c.status] ?? '#8b8b95' }}>{STATUS_L[c.status] ?? c.status}</span>
        <span className="text-[11.5px] text-mut">{c.type.replace('relation:', '')} · {dayFmt(c.asserted_at)} · {WHO[c.speaker] ?? c.speaker}</span>
      </div>
      <div className="mt-1.5 text-[13.5px] leading-snug">{c.text}</div>
      {ev.slice(0, 2).map((v: any, i: number) => (
        <div key={i} className="mt-1.5 border-l-2 border-line2 pl-2.5 text-[12px] leading-snug text-soft">
          “{v.quote}”
          <div className="mt-0.5 text-[11px] text-mut">{v.stated === 'inferred' ? 'inferred · ' : ''}<Link className="underline decoration-line2" to={`/source/${v.source_id}`}>{v.source_title ?? v.source}</Link></div>
        </div>
      ))}
      {showEntities && c.entities && <div className="mt-1.5 truncate text-[11px] text-mut">{c.entities}</div>}
    </div>
  )
}

export default function Graph() {
  const { data: lin } = useApi('/api/kg/lineage')
  const { data: sug, reload: reloadSug } = useApi('/api/kg/suggest') as { data: any; reload: () => void }
  const { data: g, reload: reloadG } = useApi('/api/kg/graph') as { data: any; reload: () => void }
  const [sp] = useSearchParams()
  const [sel, setSel] = useState('')
  const [q, setQ] = useState('')
  const [status, setStatus] = useState('')
  const [speaker, setSpeaker] = useState('')
  const [hover, setHover] = useState('')
  const [view, setView] = useState({ x: 0, y: 0, k: 1 })
  const svg = useRef<SVGSVGElement>(null)
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null)
  const { data: det } = useApi(`/api/kg/claims?entity=${encodeURIComponent(sel)}&q=${encodeURIComponent(q)}&status=${status}&speaker=${speaker}`)

  const nodes = useMemo(() => (g?.nodes ?? []).filter((n: any) => n.claims > 0), [g])
  const pos = useMemo(() => (g ? layout(nodes, g.edges) : new Map()), [g, nodes])
  useEffect(() => { const e = sp.get('e'); if (e && nodes.length) { const n = nodes.find((x: any) => x.name.toLowerCase() === e.toLowerCase()); if (n) setSel(n.id) } }, [sp, nodes])
  const cur = nodes.find((n: any) => n.id === sel)
  const byId = useMemo(() => new Map<string, any>(nodes.map((n: any) => [n.id, n])), [nodes])
  const focus = hover || sel
  const nbr = useMemo(() => {
    const m = new Map<string, number>()
    if (focus) for (const e of g?.edges ?? []) { if (e.a === focus) m.set(e.b, e.w); if (e.b === focus) m.set(e.a, e.w) }
    return m
  }, [g, focus])
  const aliases = cur ? (g?.nodes.find((n: any) => n.id === cur.id)?.aliases ?? '') : ''
  const addEntity = async (name: string, kind: string, aliasOf = '') => { await post('/api/kg/entity', { name, kind, alias_of: aliasOf }); reloadSug(); reloadG() }
  const entLineage = (lin ?? []).filter((l: any) => !cur || (l.path ?? '').split(' → ').includes(cur.name))

  useEffect(() => {
    const el = svg.current; if (!el) return
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const r = el.getBoundingClientRect(), px = ((e.clientX - r.left) / r.width) * W, py = ((e.clientY - r.top) / r.height) * H
      setView((v) => { const k = Math.min(3, Math.max(0.5, v.k * (e.deltaY < 0 ? 1.12 : 1 / 1.12))); return { k, x: px - ((px - v.x) / v.k) * k, y: py - ((py - v.y) / v.k) * k } })
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setSel('') }
    window.addEventListener('keydown', onKey)
    return () => { el.removeEventListener('wheel', onWheel); window.removeEventListener('keydown', onKey) }
  }, [])

  const zoom = (f: number) => setView((v) => { const k = Math.min(3, Math.max(0.5, v.k * f)); return { k, x: W / 2 - ((W / 2 - v.x) / v.k) * k, y: H / 2 - ((H / 2 - v.y) / v.k) * k } })
  const Chip = ({ v, cur: c, set, label }: any) => <button onClick={() => set(c === v ? '' : v)} className="rounded-full px-2.5 text-[12px] font-medium transition-colors" style={{ height: 26, border: '1px solid var(--color-line2)', background: c === v ? '#34d39926' : 'rgba(255,255,255,.04)', color: c === v ? '#34d399' : 'var(--color-soft)' }}>{label}</button>

  return (
    <div className="nx-gmap">
      <svg ref={svg} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet" className="nx-canvas select-none" style={{ cursor: drag.current ? 'grabbing' : 'grab' }}
        onMouseDown={(e) => { drag.current = { x: e.clientX, y: e.clientY, moved: false } }}
        onMouseMove={(e) => { const d = drag.current; if (!d) return; const r = svg.current!.getBoundingClientRect(); const dx = ((e.clientX - d.x) / r.width) * W, dy = ((e.clientY - d.y) / r.height) * H; if (Math.abs(dx) + Math.abs(dy) > 1) d.moved = true; d.x = e.clientX; d.y = e.clientY; setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy })) }}
        onMouseUp={() => { const m = drag.current?.moved; drag.current = null; if (!m) setSel('') }} onMouseLeave={() => { drag.current = null }}>
        <defs>
          {Object.entries(KIND_C).map(([k, c]) => (
            <radialGradient key={k} id={`g-${k}`} cx="35%" cy="30%"><stop offset="0%" stopColor="#fff" stopOpacity=".55" /><stop offset="45%" stopColor={c} /><stop offset="100%" stopColor={c} stopOpacity=".75" /></radialGradient>
          ))}
        </defs>
        <g transform={`translate(${view.x},${view.y}) scale(${view.k})`}>
          {(g?.edges ?? []).map((e: any, i: number) => {
            const a = pos.get(e.a), b = pos.get(e.b); if (!a || !b) return null
            const on = focus && (e.a === focus || e.b === focus)
            return <line key={i} x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke={on ? '#34d399' : '#a1a1aa'} strokeOpacity={on ? 0.85 : focus ? 0.05 : 0.16} strokeWidth={(on ? 1.4 : 0.8) + Math.log(1 + e.w) * 0.8} />
          })}
          {nodes.map((n: any) => {
            const p = pos.get(n.id); if (!p) return null
            const r = 9 + Math.sqrt(n.claims) * 3.2, on = n.id === sel, lit = !focus || n.id === focus || nbr.has(n.id)
            const c = KIND_C[n.kind] ?? '#8b8b95'
            return (
              <g key={n.id} transform={`translate(${p.x},${p.y})`} style={{ cursor: 'pointer', opacity: lit ? 1 : 0.22, transition: 'opacity .18s' }}
                onMouseDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); setSel(on ? '' : n.id) }} onMouseEnter={() => setHover(n.id)} onMouseLeave={() => setHover('')}>
                <circle r={r + 12} fill={c} opacity={on ? 0.28 : 0.1} />
                <circle r={r} fill={`url(#g-${n.kind in KIND_C ? n.kind : 'project'})`} stroke={on ? '#fff' : 'rgba(255,255,255,.25)'} strokeWidth={on ? 2.5 : 1} />
                <text y={r + 17} textAnchor="middle" fontSize={on ? 15 : 13} fontWeight={on ? 650 : 500} fill={on ? '#fff' : '#d4d4d8'} style={{ paintOrder: 'stroke', stroke: '#0a0a0b', strokeWidth: 4 }}>{n.name.replace(/ \(.*\)/, '')}</text>
                <text y={r + 31} textAnchor="middle" fontSize="11" fill="#8b8b95" style={{ paintOrder: 'stroke', stroke: '#0a0a0b', strokeWidth: 3 }}>{n.claims} claims</text>
              </g>
            )
          })}
        </g>
      </svg>

      <div className="absolute left-5 top-5 flex max-w-[calc(100%-480px)] flex-col gap-2.5">
        <div><div className="text-[13px] text-mut">Entities and the claims about them</div><h2 className="text-[28px] font-semibold leading-tight tracking-tight">Graph</h2></div>
        <div className="flex flex-wrap items-center gap-1.5">
          <div className="relative"><SearchIcon size={14} className="absolute left-3 top-[11px] text-mut" /><input className="nx-input !h-9 !w-[240px] !pl-8 !text-[13px]" placeholder="Search claims and quotes" value={q} onChange={(e) => setQ(e.target.value)} /></div>
          <Chip v="active" cur={status} set={setStatus} label="Kept" /><Chip v="proposed" cur={status} set={setStatus} label="Suggested" /><Chip v="superseded" cur={status} set={setStatus} label="Replaced" />
          <Chip v="owner" cur={speaker} set={setSpeaker} label="You" /><Chip v="assistant" cur={speaker} set={setSpeaker} label="Claude" />
        </div>
        <div className="flex items-center gap-3 text-[11.5px] text-mut">{Object.entries(KIND_C).map(([k, c]) => <span key={k} className="flex items-center gap-1.5"><span className="size-2 rounded-full" style={{ background: c }} />{k}</span>)}<span>· size = claims · line = claims about both</span></div>
      </div>

      <div className="absolute bottom-5 left-5 flex flex-col gap-1.5">
        {[[Plus, () => zoom(1.25), 'Zoom in'], [Minus, () => zoom(1 / 1.25), 'Zoom out'], [Maximize2, () => setView({ x: 0, y: 0, k: 1 }), 'Reset view']].map(([I, f, t]: any) => (
          <button key={t} title={t} onClick={f} className="grid size-8 place-items-center rounded-[10px] border border-line2 bg-[#18181c]/90 text-soft transition hover:bg-[#222226] active:scale-95"><I size={15} /></button>
        ))}
      </div>

      <aside className="absolute bottom-5 right-5 top-5 flex w-[420px] max-w-[calc(100%-40px)] flex-col overflow-hidden rounded-[22px] border border-line2 bg-[#141417]/95 shadow-[0_24px_60px_rgba(0,0,0,.5)] backdrop-blur-md">
        <div className="border-b border-line px-5 pb-3 pt-4">
          {cur ? (
            <>
              <div className="flex items-start gap-2">
                <div className="min-w-0 flex-1"><div className="truncate text-[19px] font-semibold tracking-tight">{cur.name}</div>
                  <div className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[12px] text-mut"><span className="size-2 rounded-full" style={{ background: KIND_C[cur.kind] ?? '#8b8b95' }} />{cur.kind} · {cur.claims} claims{aliases ? ` · also ${aliases}` : ''}</div></div>
                <button onClick={() => setSel('')} title="Close (Esc)" className="grid size-7 place-items-center rounded-lg text-mut hover:bg-white/10"><X size={15} /></button>
              </div>
              {nbr.size > 0 && (
                <div className="mt-2.5 flex flex-wrap gap-1.5">
                  {[...nbr.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10).map(([id, w]) => (
                    <button key={id} onClick={() => setSel(id)} className="rounded-full border border-line2 bg-white/[.04] px-2 text-[11.5px] text-soft hover:bg-white/10" style={{ height: 24 }}>{byId.get(id)?.name.replace(/ \(.*\)/, '')} <span className="text-mut">{w}</span></button>
                  ))}
                </div>
              )}
            </>
          ) : (
            <div><div className="text-[17px] font-semibold tracking-tight">All claims</div><div className="mt-0.5 text-[12px] text-mut">Click a node to see what was said about it, or search above.</div></div>
          )}
        </div>
        <div className="flex-1 overflow-y-auto px-4 py-3">
          {entLineage.length > 0 && (
            <div className="mb-4">
              <div className="mb-1.5 text-[11.5px] font-semibold uppercase tracking-wide text-mut">Lineage{cur ? ` of ${cur.name.replace(/ \(.*\)/, '')}` : ''}</div>
              <div className="flex flex-col gap-1.5">
                {entLineage.slice(0, cur ? 20 : 8).map((l: any) => (
                  <div key={l.id} className="rounded-[12px] border border-line bg-white/[.025] px-3 py-2">
                    <div className="flex flex-wrap items-baseline gap-x-2 text-[12.5px]"><span className="tabular-nums text-mut">{dayFmt(l.asserted_at)}</span><span>{l.path}</span><span className="text-mut">{l.type.replace('relation:', '')}</span></div>
                    <div className="mt-0.5 text-[11.5px] leading-snug text-soft">“{l.quote}”</div>
                  </div>
                ))}
              </div>
            </div>
          )}
          {!cur && sug && (sug.entities.length > 0 || sug.aliases.length > 0) && (
            <details className="mb-4 rounded-[12px] border border-line bg-white/[.025] px-3 py-2">
              <summary className="cursor-pointer text-[12px] font-semibold uppercase tracking-wide text-mut">Suggested entities · {sug.entities.length + sug.aliases.length}</summary>
              <div className="mt-2 text-[11.5px] text-mut">Names found often in what you wrote that are not entities yet. Add one to link its claims.</div>
              {sug.aliases.map((a: any) => (
                <div key={a.alias} className="mt-1.5 flex items-center gap-2 text-[12.5px]"><span className="flex-1 truncate">{a.alias} <span className="text-mut">→ {a.entity} · {a.turns}</span></span><button className="btn sm" onClick={() => addEntity(a.alias, 'project', a.entity)}>Add alias</button></div>
              ))}
              {sug.entities.slice(0, 12).map((e: any) => (
                <div key={e.term} className="mt-1.5 flex items-center gap-2 text-[12.5px]"><span className="flex-1 truncate">{e.term} <span className="text-mut">{e.turns} turns</span></span>
                  {['project', 'service', 'tool'].map((k) => <button key={k} className="btn sm" onClick={() => addEntity(e.term, k)}>{k}</button>)}</div>
              ))}
            </details>
          )}
          <div className="mb-1.5 text-[11.5px] font-semibold uppercase tracking-wide text-mut">Claims · {det?.total ?? '…'}{det && det.total > 150 ? ' (newest 150)' : ''}</div>
          <div className="flex flex-col gap-2">
            {(det?.claims ?? []).map((c: any) => <ClaimCard key={c.id} c={c} ev={(det.evidence ?? []).filter((v: any) => v.claim_id === c.id)} showEntities={!cur} />)}
            {det && det.claims.length === 0 && <div className="text-[13px] text-mut">Nothing matches.</div>}
          </div>
        </div>
      </aside>
    </div>
  )
}
