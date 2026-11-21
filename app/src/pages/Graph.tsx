import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { Minus, Plus, Search as SearchIcon, Sparkles, User, X, Maximize2, PanelRightClose, PanelRightOpen } from 'lucide-react'
import { dayFmt, post, useApi } from '@/lib/api'
import Trail from '@/pages/Trail'
import DecisionGraph from '@/pages/DecisionGraph'
import More from '@/components/More'
import Paged from '@/components/Paged'
import Topics, { KIND_C, KINDS } from '@/components/Topics'
import type { LucideIcon } from 'lucide-react'
import type { GraphNode, KgClaim, KgClaims, KgEvidence, KgGraph, KgSuggest, LineageClaim } from '@/lib/types'
import '@/next.css'

const STATUS_C: Record<string, string> = { active: '#34d399', proposed: '#8b8b95', superseded: '#fbbf24', retracted: '#fb7185' }
const STATUS_L: Record<string, string> = { active: 'kept', proposed: 'suggested', superseded: 'replaced', retracted: 'rejected' }
const WHO: Record<string, string> = { owner: 'You', assistant: 'Claude', unknown: 'Unknown' }
const W = 1300, H = 1000, SX = W / 880, SY = H / 700

function layout(nodes: GraphNode[], edges: KgGraph['edges']) {
  const pos = new Map<string, { x: number; y: number }>()
  const topics = nodes.filter((n) => n.kind === 'topic'), main = nodes.filter((n) => n.kind !== 'topic')
  const isTopic = new Set(topics.map((n) => n.id))
  main.forEach((n, i) => { const a = (i / main.length) * Math.PI * 2; pos.set(n.id, { x: W / 2 + Math.cos(a) * 200 * SX, y: H / 2 + Math.sin(a) * 170 * SY }) })
  topics.forEach((n, i) => { const a = (i / topics.length) * Math.PI * 2 - Math.PI / 2; pos.set(n.id, { x: W / 2 + Math.cos(a) * 390 * SX, y: H / 2 + 30 + Math.sin(a) * 285 * SY }) })
  edges = edges.filter((e) => !isTopic.has(e.a) && !isTopic.has(e.b))
  for (let it = 0; it < 360; it++) {
    const f = new Map<string, { x: number; y: number }>(nodes.map((n) => [n.id, { x: 0, y: 0 }]))
    for (let i = 0; i < nodes.length; i++) for (let j = i + 1; j < nodes.length; j++) {
      const a = pos.get(nodes[i].id)!, b = pos.get(nodes[j].id)!
      const dx = a.x - b.x, dy = a.y - b.y, d2 = Math.max(dx * dx + dy * dy, 60), k = 17000 * SX * SX / d2, d = Math.sqrt(d2)
      f.get(nodes[i].id)!.x += (dx / d) * k; f.get(nodes[i].id)!.y += (dy / d) * k
      f.get(nodes[j].id)!.x -= (dx / d) * k; f.get(nodes[j].id)!.y -= (dy / d) * k
    }
    for (const e of edges) {
      const a = pos.get(e.a), b = pos.get(e.b); if (!a || !b) continue
      const dx = b.x - a.x, dy = b.y - a.y, d = Math.sqrt(dx * dx + dy * dy) || 1, k = (d - 165 * SX) * 0.008 * Math.min(1 + Math.log(e.w), 3)
      f.get(e.a)!.x += (dx / d) * k; f.get(e.a)!.y += (dy / d) * k; f.get(e.b)!.x -= (dx / d) * k; f.get(e.b)!.y -= (dy / d) * k
    }
    for (const n of main) {
      const p = pos.get(n.id)!, g = f.get(n.id)!
      g.x += (W / 2 - p.x) * 0.0035; g.y += (H / 2 - p.y) * 0.007
      p.x = Math.min(W - 70, Math.max(70, p.x + Math.max(-10, Math.min(10, g.x)))); p.y = Math.min(H - 50, Math.max(40, p.y + Math.max(-10, Math.min(10, g.y))))
    }
  }
  return pos
}

function ClaimCard({ c, ev, showEntities, kindOf, onPick }: { c: KgClaim; ev: KgEvidence[]; showEntities?: boolean; kindOf: (n: string) => string | undefined; onPick: (n: string) => void }) {
  const col = STATUS_C[c.status] ?? '#8b8b95'
  const names: string[] = showEntities && c.entities ? String(c.entities).split(' | ').filter(Boolean) : []
  return (
    <div className="rounded-[14px] border border-line bg-white/[.025] py-2.5 pl-3.5 pr-3" style={{ borderLeft: `3px solid ${col}` }}>
      <div className="flex items-center gap-2 text-[11.5px] text-mut">
        <span className="tabular-nums">{dayFmt(c.asserted_at)}</span>
        <span className="inline-flex items-center gap-1 rounded-full bg-white/[.06] px-1.5 py-px text-soft">{c.speaker === 'owner' ? <User size={10} /> : <Sparkles size={10} />}{WHO[c.speaker] ?? c.speaker}</span>
        <span className="ml-auto inline-flex items-center gap-1" style={{ color: col }}><span className="size-1.5 rounded-full" style={{ background: col }} />{STATUS_L[c.status] ?? c.status}</span>
      </div>
      <div className={`mt-1.5 line-clamp-3 text-[14px] leading-snug ${c.status === 'superseded' ? 'text-soft' : ''}`}>{c.text}</div>
      {names.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1">
          {names.slice(0, 4).map((n) => (
            <button key={n} onClick={() => onPick(n)} className="inline-flex items-center gap-1 rounded-full border border-line2 px-2 py-px text-[11.5px] text-soft hover:bg-white/10"><span className="size-1.5 rounded-full" style={{ background: KIND_C[kindOf(n) ?? ''] ?? '#8b8b95' }} />{n.replace(/ \(.*\)/, '')}</button>
          ))}
        </div>
      )}
      {ev.length > 0 && <More label="Quote" className="mt-1.5">{ev.slice(0, 2).map((v, i) => (
        <div key={i} className="mt-1.5 border-l-2 border-line2 pl-2.5 text-[12px] leading-snug text-soft">
          “{v.quote}”
          <div className="mt-0.5 text-[11px] text-mut">{v.stated === 'inferred' ? 'inferred · ' : ''}<Link className="underline decoration-line2" to={`/source/${v.source_id}`}>{v.source_title ?? v.source}</Link></div>
        </div>
      ))}</More>}
    </div>
  )
}

function Network({ tabs }: { tabs: React.ReactNode }) {
  const { data: lin } = useApi<LineageClaim[]>('/api/kg/lineage')
  const { data: sug, reload: reloadSug } = useApi<KgSuggest>('/api/kg/suggest')
  const { data: g, reload: reloadG } = useApi<KgGraph>('/api/kg/graph')
  const [sp] = useSearchParams()
  const [sel, setSel] = useState('')
  const [q, setQ] = useState('')
  const [status, setStatus] = useState('')
  const [speaker, setSpeaker] = useState('')
  const [hover, setHover] = useState('')
  const [open, setOpen] = useState(true)
  const [view, setView] = useState({ x: 0, y: 0, k: 1 })
  const svg = useRef<SVGSVGElement>(null)
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null)
  const { data: det } = useApi<KgClaims>(`/api/kg/claims?entity=${encodeURIComponent(sel)}&q=${encodeURIComponent(q)}&status=${status}&speaker=${speaker}`)

  const nodes = useMemo(() => (g?.nodes ?? []).filter((n) => n.claims > 0), [g])
  const pos = useMemo(() => (g ? layout(nodes, g.edges) : new Map<string, { x: number; y: number }>()), [g, nodes])
  useEffect(() => { const e = sp.get('e'); if (e && nodes.length) { const n = nodes.find((x) => x.name.toLowerCase() === e.toLowerCase()); if (n) setSel(n.id) } }, [sp, nodes])
  const cur = nodes.find((n) => n.id === sel)
  const byId = useMemo(() => new Map<string, GraphNode>(nodes.map((n) => [n.id, n])), [nodes])
  useEffect(() => { if (sel) setOpen(true) }, [sel])
  const focus = hover || sel
  const nbr = useMemo(() => {
    const m = new Map<string, number>()
    if (focus) for (const e of g?.edges ?? []) { if (e.a === focus) m.set(e.b, e.w); if (e.b === focus) m.set(e.a, e.w) }
    return m
  }, [g, focus])
  const aliases = cur ? (g?.nodes.find((n) => n.id === cur.id)?.aliases ?? '') : ''
  const [tab, setTab] = useState(sp.get('tab') ?? 'claims')
  useEffect(() => { const t = sp.get('tab'); if (t) setTab(t) }, [sp])
  const toReview = sug ? sug.entities.length + sug.aliases.length : 0
  const kindOfName = (n: string) => nodes.find((x) => x.name === n)?.kind
  const pickByName = (n: string) => { const m = nodes.find((x) => x.name === n); if (m) { setSel(m.id); setTab('claims') } }
  const entLineage = (lin ?? []).filter((l) => !cur || (l.path ?? '').split(' → ').includes(cur.name))

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
  const Chip = ({ v, cur: c, set, label, dot }: { v: string; cur: string; set: (v: string) => void; label: string; dot?: string }) => <button onClick={() => set(c === v ? '' : v)} className="inline-flex items-center gap-1.5 rounded-full px-2 text-[12px] font-medium transition-colors" style={{ height: 26, border: '1px solid var(--color-line2)', background: c === v ? '#34d39926' : 'rgba(255,255,255,.04)', color: c === v ? '#34d399' : 'var(--color-soft)' }}>{dot && <span className="size-1.5 rounded-full" style={{ background: dot }} />}{label}</button>
  const shownTab = cur && tab === 'topics' ? 'claims' : tab

  return (
    <div className="nx-gmap">
      <svg ref={svg} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet" className={`nx-canvas select-none ${open ? '' : 'nx-wide'}`} style={{ cursor: drag.current ? 'grabbing' : 'grab' }}
        onMouseDown={(e) => { drag.current = { x: e.clientX, y: e.clientY, moved: false } }}
        onMouseMove={(e) => { const d = drag.current; if (!d) return; const r = svg.current!.getBoundingClientRect(); const dx = ((e.clientX - d.x) / r.width) * W, dy = ((e.clientY - d.y) / r.height) * H; if (Math.abs(dx) + Math.abs(dy) > 1) d.moved = true; d.x = e.clientX; d.y = e.clientY; setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy })) }}
        onMouseUp={() => { const m = drag.current?.moved; drag.current = null; if (!m) setSel('') }} onMouseLeave={() => { drag.current = null }}>
        <defs>
          {Object.entries(KIND_C).map(([k, c]) => (
            <radialGradient key={k} id={`g-${k}`} cx="35%" cy="30%"><stop offset="0%" stopColor="#fff" stopOpacity=".55" /><stop offset="45%" stopColor={c} /><stop offset="100%" stopColor={c} stopOpacity=".75" /></radialGradient>
          ))}
        </defs>
        <g transform={`translate(${view.x},${view.y}) scale(${view.k})`}>
          {(g?.edges ?? []).map((e, i) => {
            const a = pos.get(e.a), b = pos.get(e.b); if (!a || !b) return null
            const on = focus && (e.a === focus || e.b === focus)
            if (!on && (byId.get(e.a)?.kind === 'topic' || byId.get(e.b)?.kind === 'topic')) return null
            return <line key={i} x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke={on ? '#34d399' : '#a1a1aa'} strokeOpacity={on ? 0.85 : focus ? 0.05 : 0.16} strokeWidth={(on ? 1.4 : 0.8) + Math.log(1 + e.w) * 0.8} />
          })}
          {nodes.map((n) => {
            const p = pos.get(n.id); if (!p) return null
            const r = Math.min(46, (9 + Math.sqrt(n.claims) * 2.1) * 1.15), on = n.id === sel, lit = !focus || n.id === focus || nbr.has(n.id)
            const c = KIND_C[n.kind] ?? '#8b8b95'
            return (
              <g key={n.id} transform={`translate(${p.x},${p.y})`} style={{ cursor: 'pointer', opacity: lit ? 1 : 0.22, transition: 'opacity .18s' }}
                onMouseDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); setSel(on ? '' : n.id) }} onMouseEnter={() => setHover(n.id)} onMouseLeave={() => setHover('')}>
                <circle r={r + 12} fill={c} opacity={on ? 0.28 : 0.1} />
                <circle r={r} fill={`url(#g-${n.kind in KIND_C ? n.kind : 'project'})`} stroke={on ? '#fff' : 'rgba(255,255,255,.25)'} strokeWidth={on ? 2.5 : 1} />
                <text y={r + 21} textAnchor="middle" fontSize={on ? 21 : 18} fontWeight={on ? 650 : 500} fill={on ? '#fff' : '#d4d4d8'} style={{ paintOrder: 'stroke', stroke: '#0a0a0b', strokeWidth: 4 }}>{n.name.replace(/ \(.*\)/, '')}</text>
                <text y={r + 38} textAnchor="middle" fontSize="14" fill="#8b8b95" style={{ paintOrder: 'stroke', stroke: '#0a0a0b', strokeWidth: 3 }}>{n.claims} claims</text>
              </g>
            )
          })}
        </g>
      </svg>

      <div className="absolute left-5 top-5 flex max-w-[calc(100%-420px)] flex-col gap-2.5">
        <div><div className="text-[13px] text-mut">Entities from sessions and notes, with the claims filed about each</div><div className="flex items-center gap-4"><h2 className="text-[28px] font-semibold leading-tight tracking-tight">Graph</h2>{tabs}</div></div>
        <div className="flex items-center gap-3 text-[11.5px] text-mut">{Object.entries(KIND_C).map(([k, c]) => <span key={k} className="flex items-center gap-1.5"><span className="size-2 rounded-full" style={{ background: c }} />{k}</span>)}<span>· size = claims · line = claims about both</span></div>
      </div>

      <div className="absolute bottom-5 left-5 flex flex-col gap-1.5">
        {([[Plus, () => zoom(1.25), 'Zoom in'], [Minus, () => zoom(1 / 1.25), 'Zoom out'], [Maximize2, () => setView({ x: 0, y: 0, k: 1 }), 'Reset view']] as [LucideIcon, () => void, string][]).map(([I, f, t]) => (
          <button key={t} title={t} onClick={f} className="grid size-8 place-items-center rounded-[10px] border border-line2 bg-[#18181c]/90 text-soft transition hover:bg-[#222226] active:scale-95"><I size={15} /></button>
        ))}
      </div>

      <button title={open ? 'Hide panel' : 'Show panel'} onClick={() => setOpen((v) => !v)} className="absolute right-5 top-5 z-10 grid size-8 place-items-center rounded-[10px] border border-line2 bg-[#18181c]/90 text-soft transition hover:bg-[#222226]" style={{ display: open ? 'none' : undefined }}><PanelRightOpen size={15} /></button>

      <aside style={{ display: open ? undefined : 'none' }} className="absolute bottom-5 right-5 top-5 flex w-[360px] max-w-[calc(100%-40px)] flex-col overflow-hidden rounded-[22px] border border-line2 bg-[#141417]/95 shadow-[0_24px_60px_rgba(0,0,0,.5)] backdrop-blur-md">
        <div className="border-b border-line px-4 pb-3 pt-3.5">
          {cur && (
            <div className="mb-3">
              <div className="flex items-start gap-2">
                <div className="min-w-0 flex-1"><div className="truncate text-[19px] font-semibold tracking-tight">{cur.name}</div>
                  <div className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[12px] text-mut"><span className="size-2 rounded-full" style={{ background: KIND_C[cur.kind] ?? '#8b8b95' }} /><select value={cur.kind} title="Change the kind" onChange={async (e) => { await post('/api/kg/entity', { name: cur.name, kind: e.target.value }); reloadG() }} className="cursor-pointer rounded bg-transparent text-[12px] text-mut hover:text-ink">{[...new Set([cur.kind, ...KINDS.map(([k]) => k)])].map((k) => <option key={k} value={k} className="bg-[#141417]">{k}</option>)}</select> · {cur.claims} claims{aliases ? ` · also ${aliases}` : ''}</div></div>
                <button onClick={() => setSel('')} title="Deselect (Esc)" className="grid size-7 place-items-center rounded-lg text-mut hover:bg-white/10"><X size={15} /></button>
              </div>
              {nbr.size > 0 && (
                <div className="mt-2.5 flex flex-wrap gap-1.5">
                  {[...nbr.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10).map(([id, w]) => (
                    <button key={id} onClick={() => setSel(id)} className="inline-flex items-center gap-1 rounded-full border border-line2 bg-white/[.04] px-2 text-[11.5px] text-soft hover:bg-white/10" style={{ height: 24 }}><span className="size-1.5 rounded-full" style={{ background: KIND_C[byId.get(id)?.kind ?? ''] ?? '#8b8b95' }} />{byId.get(id)?.name.replace(/ \(.*\)/, '')} <span className="text-mut">{w}</span></button>
                  ))}
                </div>
              )}
            </div>
          )}
          <div className="flex items-center gap-1.5"><div className="flex flex-1 gap-1 rounded-[11px] bg-white/[.05] p-1">
            {([['claims', 'Claims', det?.total ?? ''], ['lineage', 'Lineage', entLineage.length || ''], ...(cur ? [] : [['topics', 'Topics', toReview || '']])] as [string, string, number | ''][]).map(([k, l, n]) => (
              <button key={k} type="button" onClick={() => setTab(k)} className="flex h-8 flex-1 items-center justify-center gap-1.5 rounded-[8px] text-[13px] font-medium transition-colors" style={{ background: shownTab === k ? '#2a2a30' : 'transparent', color: shownTab === k ? '#fff' : 'var(--color-soft)' }}>
                {l}{n !== '' && <span className="rounded-full px-1.5 text-[11px] tabular-nums" style={k === 'topics' ? { background: '#fbbf2426', color: '#fbbf24' } : { color: 'var(--color-mut)' }}>{n}</span>}
              </button>
            ))}
          </div><button title="Hide panel" onClick={() => setOpen(false)} className="grid size-8 shrink-0 place-items-center rounded-[10px] text-mut hover:bg-white/10"><PanelRightClose size={15} /></button></div>
        </div>
        <div className="flex-1 overflow-y-auto px-4 py-3">
          {shownTab === 'topics' && <Topics sug={sug} onChanged={() => { reloadSug(); reloadG(); window.dispatchEvent(new Event('bt-suggest')) }} />}
          {shownTab === 'lineage' && (
            <div className="flex flex-col gap-2">
              {entLineage.length === 0 && <div className="text-[13px] text-mut">No lineage recorded{cur ? ' for this one' : ''}.</div>}
              {entLineage.slice(0, 40).map((l) => (
                <div key={l.id} className="rounded-[14px] border border-line bg-white/[.025] px-3 py-2.5">
                  <div className="flex flex-wrap items-center gap-1.5 text-[13px]">
                    {(l.path ?? '').split(' → ').map((n, i) => (
                      <span key={i} className="inline-flex items-center gap-1.5">{i > 0 && <span className="text-mut">→</span>}<button onClick={() => pickByName(n)} className="inline-flex items-center gap-1 rounded-full border border-line2 px-2 py-px hover:bg-white/10"><span className="size-1.5 rounded-full" style={{ background: KIND_C[kindOfName(n) ?? ''] ?? '#8b8b95' }} />{n.replace(/ \(.*\)/, '')}</button></span>
                    ))}
                  </div>
                  <div className="mt-1.5 flex items-center gap-2 text-[11.5px] text-mut"><span className="rounded-full bg-white/[.06] px-1.5 py-px text-soft">{l.type.replace('relation:', '')}</span><span className="tabular-nums">{dayFmt(l.asserted_at)}</span></div>
                  <More label="Quote" className="mt-1 text-[11.5px] leading-snug text-soft">“{l.quote}”</More>
                </div>
              ))}
            </div>
          )}
          {shownTab === 'claims' && (
            <>
              <div className="mb-3 grid gap-2">
                <div className="relative"><SearchIcon size={14} className="absolute left-3 top-[11px] text-mut" /><input className="nx-input !h-9 !w-full !pl-8 !text-[13px]" placeholder="Search the claims" value={q} onChange={(e) => setQ(e.target.value)} /></div>
                <div className="flex flex-wrap items-center gap-1.5">
                  <Chip v="active" cur={status} set={setStatus} label="Kept" dot={STATUS_C.active} /><Chip v="proposed" cur={status} set={setStatus} label="Suggested" dot={STATUS_C.proposed} /><Chip v="superseded" cur={status} set={setStatus} label="Replaced" dot={STATUS_C.superseded} />
                  <span className="mx-1 h-4 w-px bg-line2" />
                  <Chip v="owner" cur={speaker} set={setSpeaker} label="You" /><Chip v="assistant" cur={speaker} set={setSpeaker} label="Claude" />
                </div>
              </div>
              <div className="flex flex-col gap-2">
                <Paged resetKey={`${sel}|${q}|${status}|${speaker}`} items={det?.claims ?? []} size={8}>{(pageRows) => pageRows.map((c) => <ClaimCard key={c.id} c={c} ev={(det?.evidence ?? []).filter((v) => v.claim_id === c.id)} showEntities={!cur} kindOf={kindOfName} onPick={pickByName} />)}</Paged>
                {det && det.claims.length === 0 && <div className="text-[13px] text-mut">Nothing matches.</div>}
              </div>
            </>
          )}
        </div>
      </aside>
    </div>
  )
}

export default function Graph() {
  const [sp, setSp] = useSearchParams()
  const view = sp.get('view') ?? ''
  const tabs = (
    <div className="flex gap-1.5">
      {([['', 'Decisions'], ['mentions', 'Mentions'], ['trail', 'Trail']] as const).map(([k, l]) => (
        <button key={k} type="button" onClick={() => { const n = new URLSearchParams(); if (k) n.set('view', k); setSp(n) }} className="rounded-full px-3 text-[12.5px] font-medium" style={{ height: 28, border: '1px solid var(--color-line2)', background: view === k ? '#34d39926' : 'rgba(255,255,255,.04)', color: view === k ? '#34d399' : 'var(--color-soft)' }}>{l}</button>
      ))}
    </div>
  )
  if (view === 'trail') return <Trail tabs={tabs} />
  if (view === 'mentions' || sp.get('e') || sp.get('tab')) return <Network tabs={tabs} />
  return <DecisionGraph tabs={tabs} />
}
