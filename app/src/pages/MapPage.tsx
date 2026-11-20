import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { Background, Controls, Handle, Position, ReactFlow, type Edge, type Node, type NodeChange, type NodeProps } from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { ArrowLeft, Bug, CircleHelp, ExternalLink, FileText, ListTodo, Plus, Scale, Sparkles, X } from 'lucide-react'
import { post, STATE, useApi } from '@/lib/api'
import { useShell } from '@/components/Shell'
import type { LucideIcon } from 'lucide-react'
import type { Done, Focus, FocusNote, Item, ThreadStat } from '@/lib/types'

type Mark = { dim?: boolean; sel?: boolean }
type CenterData = Partial<ThreadStat> & { notes: number; sessions: number; commits: number }
type NoteData = FocusNote & Mark
type ItemData = Item & Mark
type LinkedData = Focus['linked'][number] & Mark & { state?: string }
type Sel = { kind: 'note'; id: number } | { kind: 'item'; id: string }

const hidden = { opacity: 0, width: 1, height: 1, border: 0 }
const H = () => (<><Handle type="target" position={Position.Left} style={hidden} /><Handle type="source" position={Position.Right} style={hidden} /></>)
const stateColor = (s: string) => STATE[s]?.color ?? '#52525b'

function CenterCard({ data: d }: NodeProps<Node<CenterData>>) {
  return (
    <div className="relative w-[320px] overflow-hidden rounded-[26px] border border-line2 bg-gradient-to-b from-[#1c1c20] to-[#121214] p-5 shadow-[0_24px_60px_rgba(0,0,0,.55),inset_0_1px_0_rgba(255,255,255,.06)]">
      <H />
      <div className="pointer-events-none absolute -right-16 -top-20 size-56 rounded-full" style={{ background: `radial-gradient(circle, ${stateColor(d.state ?? '')}40, transparent 65%)` }} />
      <div className="relative flex items-center gap-3"><span className="nx-glyph size-11 text-[18px] font-semibold" style={{ background: `color-mix(in srgb, ${stateColor(d.state ?? '')} 16%, transparent)`, color: stateColor(d.state ?? '') }}>{d.name?.[0]}</span>
        <div className="flex items-center gap-2 text-[12px] text-mut"><span className="size-2.5 rounded-full" style={{ background: stateColor(d.state ?? '') }} />{STATE[d.state ?? '']?.label}{d.kind && <span className="chip">{d.kind}</span>}</div></div>
      <div className="relative mt-3 text-[24px] font-semibold leading-tight tracking-tight">{d.name}</div>
      <div className="relative mt-1.5 text-[13px] leading-5 text-soft">{d.summary}</div>
      <div className="relative mt-3 flex flex-wrap gap-1.5 text-[12px]"><span className="chip">{d.notes} notes and artifacts</span><span className="chip">{d.sessions} conversations</span>{d.commits > 0 && <span className="chip">{d.commits} commit lines</span>}</div>
    </div>
  )
}

function NoteCard({ data: n }: NodeProps<Node<NoteData>>) {
  return (
    <div className={`node-card w-[300px] rounded-[18px] border border-line bg-gradient-to-b from-[#171719] to-[#111113] p-3.5 shadow-[inset_0_1px_0_rgba(255,255,255,.04)] ${n.sel ? 'sel' : ''}`} style={{ opacity: n.dim ? 0.3 : 1 }}>
      <H />
      <div className="flex items-center gap-2.5"><span className="nx-glyph size-8 shrink-0" style={{ background: n.kind === 'artifact' ? '#c084fc1f' : '#8b8b951f', color: n.kind === 'artifact' ? '#c084fc' : '#a1a1aa' }}>{n.kind === 'artifact' ? <Sparkles size={15} /> : <FileText size={15} />}</span><span className="truncate text-[13.5px] font-semibold">{n.title}</span></div>
      <div className="mt-2 line-clamp-3 min-h-[48px] text-[12.5px] leading-[1.35] text-soft">{n.gist || 'No summary line yet.'}</div>
      <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[11.5px] text-mut">
        <span>{n.outline.length > 0 ? `${n.outline.length} sections` : 'one section'}</span>
        {n.cited > 0 && <span className="rounded-full bg-accent/15 px-1.5 py-0.5 text-accent">{n.cited} {n.cited === 1 ? 'item' : 'items'} cite it</span>}
        {n.links > 0 && <span>{n.links} linked notes</span>}
      </div>
    </div>
  )
}

const ITEM_ICON: Record<string, LucideIcon> = { question: CircleHelp, step: ListTodo, decision: Scale, turn: ListTodo }
function ItemCard({ data: i }: NodeProps<Node<ItemData>>) {
  const Icon = i.tag === 'bug' ? Bug : ITEM_ICON[i.type] ?? ListTodo
  const color = i.type === 'question' ? 'var(--color-open)' : i.state === 'done' ? 'var(--color-built)' : i.tag === 'bug' ? 'var(--color-dropped)' : 'var(--color-mut)'
  return (
    <div className={`node-card w-[250px] rounded-2xl border border-line bg-gradient-to-b from-[#171719] to-[#111113] px-3 py-2.5 ${i.sel ? 'sel' : ''}`} style={{ opacity: i.dim ? 0.3 : 1 }}>
      <H />
      <div className="flex items-start gap-2.5"><span className="nx-glyph mt-px size-6 shrink-0 !rounded-lg" style={{ background: `color-mix(in srgb, ${color} 16%, transparent)`, color }}><Icon size={13} /></span><span className={`line-clamp-2 text-[12.5px] leading-[1.3] ${i.state === 'done' ? 'text-mut line-through' : ''}`}>{i.title}</span></div>
      <div className="mt-1.5 flex items-center gap-1.5 text-[11px] text-mut"><span className="capitalize">{i.type}</span>{i.state === 'doing' && <span className="text-moving">doing</span>}{i.state === 'looks_done' && <span className="text-accent">looks done?</span>}{i.state === 'proposed' && <span className="text-moving">suggested</span>}</div>
    </div>
  )
}

function LinkedCard({ data: l }: NodeProps<Node<LinkedData>>) {
  return (
    <div className="node-card flex w-[210px] items-center gap-2 rounded-full border border-line2 bg-[#151517] px-3.5 py-2 shadow-[inset_0_1px_0_rgba(255,255,255,.05)]">
      <H /><span className="size-2 rounded-full" style={{ background: stateColor(l.state ?? '') }} /><span className="truncate text-[12.5px] font-medium">{l.name}</span>
      <span className="ml-auto shrink-0 text-[11px] text-mut">{l.kind === 'built on' ? (l.dir === 'out' ? 'built on' : 'builds on it') : `${l.w} mentions`}</span>
    </div>
  )
}

const nodeTypes = { center: CenterCard, note: NoteCard, item: ItemCard, linked: LinkedCard }

function focusLayout(F: Focus, sel: Sel | null, all: boolean) {
  const topLinked = [...F.linked].sort((a, b) => Number(b.kind === 'built on') - Number(a.kind === 'built on') || b.w - a.w).slice(0, 5)
  const shownNotes = all ? F.notes : F.notes.slice(0, 8)
  const ids = new Set(shownNotes.map((n) => n.id))
  const f = { ...F, linked: topLinked, notes: shownNotes, note_edges: F.note_edges.filter((e) => ids.has(e.a) && ids.has(e.b)), hidden_notes: F.hidden_notes + (F.notes.length - shownNotes.length) }
  const t = f.thread
  const nodes: Node[] = [], edges: Edge[] = []
  const cnt = (k: string) => f.counts.find((c) => c.kind === k)?.n ?? 0
  nodes.push({ id: 'center', type: 'center', position: { x: -160, y: -70 }, data: { ...t, notes: f.notes.length + f.hidden_notes, sessions: cnt('session'), commits: cnt('commit') }, draggable: false })

  const order: Record<string, number> = { question: 0, step: 1, decision: 2, turn: 3 }
  const items = [...f.items].sort((a, b) => (a.state === 'open' ? 0 : 1) - (b.state === 'open' ? 0 : 1) || order[a.type] - order[b.type])
  const shownItems = all ? items : items.slice(0, 7)
  const iy0 = -(shownItems.length * 84) / 2
  shownItems.forEach((it, i) => nodes.push({ id: `I:${it.id}`, type: 'item', position: { x: -560, y: iy0 + i * 84 }, data: it }))

  const rows = Math.ceil(f.notes.length / 2)
  const ny0 = -(rows * 168) / 2
  f.notes.forEach((n, i) => nodes.push({ id: `N:${n.id}`, type: 'note', position: { x: 300 + (i % 2) * 330, y: ny0 + Math.floor(i / 2) * 168 }, data: n }))

  f.linked.forEach((l, i) => {
    const x = -160 + (i - (f.linked.length - 1) / 2) * 215 + 60
    nodes.push({ id: `L:${l.name}:${l.kind}`, type: 'linked', position: { x, y: -(Math.max(rows * 168, 300) / 2) - 90 }, data: l })
    edges.push({ id: `l${i}`, source: 'center', target: `L:${l.name}:${l.kind}`, style: { stroke: l.kind === 'built on' ? '#60a5fa' : '#3a3a42', strokeWidth: l.kind === 'built on' ? 2 : 1.2 }, sourceHandle: null })
  })
  shownItems.forEach((it) => edges.push({ id: `ci${it.id}`, source: `I:${it.id}`, target: 'center', style: { stroke: '#2c2c32', strokeWidth: 1.2 } }))
  f.notes.forEach((n) => edges.push({ id: `cn${n.id}`, source: 'center', target: `N:${n.id}`, style: { stroke: '#2c2c32', strokeWidth: 1.2 } }))
  f.note_edges.filter((e) => sel && sel.kind === 'note' && (e.a === sel.id || e.b === sel.id)).forEach((e, i) => edges.push({ id: `w${i}`, source: `N:${e.a}`, target: `N:${e.b}`, style: { stroke: '#34d399', strokeWidth: 1.6, strokeDasharray: '5 4' }, label: 'links to', labelStyle: { fill: '#34d399', fontSize: 10.5 }, labelBgStyle: { fill: '#0a0a0b' } }))
  shownItems.forEach((it) => { if (it.source_id && f.notes.some((n) => n.id === it.source_id)) edges.push({ id: `s${it.id}`, source: `I:${it.id}`, target: `N:${it.source_id}`, style: { stroke: '#34d399', strokeWidth: 1.4, opacity: sel ? 0.95 : 0.0 } }) })

  if (sel) {
    const rel = new Set<string>(['center'])
    if (sel.kind === 'note') {
      rel.add(`N:${sel.id}`)
      f.items.filter((i) => i.source_id === sel.id).forEach((i) => rel.add(`I:${i.id}`))
      f.note_edges.forEach((e) => { if (e.a === sel.id) rel.add(`N:${e.b}`); if (e.b === sel.id) rel.add(`N:${e.a}`) })
    } else {
      rel.add(`I:${sel.id}`)
      const it = f.items.find((i) => i.id === sel.id)
      if (it?.source_id) rel.add(`N:${it.source_id}`)
    }
    nodes.forEach((n) => { n.data = { ...n.data, dim: !rel.has(n.id) && n.type !== 'center' && n.type !== 'linked', sel: n.id === `${sel.kind === 'note' ? 'N' : 'I'}:${sel.id}` } })
  }
  return { nodes, edges }
}

function Panel({ f, sel, close, reload, pick }: { f: Focus; sel: Sel; close: () => void; reload: () => void; pick: (s: Sel) => void }) {
  const [ans, setAns] = useState('')
  if (sel.kind === 'note') {
    const n = f.notes.find((x) => x.id === sel.id)
    if (!n) return null
    const cites = f.items.filter((i) => i.source_id === n.id)
    const linked = f.note_edges.filter((e) => e.a === n.id || e.b === n.id).map((e) => f.notes.find((x) => x.id === (e.a === n.id ? e.b : e.a))).filter((x): x is FocusNote => !!x)
    const linkedU = linked.filter((x, i) => linked.findIndex((y) => y.id === x.id) === i)
    return (
      <aside className="absolute right-3 top-3 z-10 flex max-h-[calc(100%-24px)] w-[340px] flex-col overflow-hidden rounded-[22px] border border-line2 bg-[#141416]/92 shadow-[0_24px_60px_rgba(0,0,0,.55),inset_0_1px_0_rgba(255,255,255,.05)] backdrop-blur-xl">
        <div className="flex items-start gap-2 p-4 pb-2"><div><div className="text-[12px] text-mut">{n.kind === 'artifact' ? 'Artifact' : 'Note'}</div><div className="text-[16px] font-semibold leading-tight">{n.title}</div></div><button className="btn ghost sm ml-auto" onClick={close}><X size={14} /></button></div>
        <div className="overflow-auto px-4 pb-4">
          <p className="text-[13px] leading-5 text-soft">{n.gist}</p>
          {n.outline.length > 0 && <><div className="mb-1 mt-4 text-[12px] font-medium text-mut">What is inside</div>
            <div className="flex flex-col">{n.outline.map((o) => <a key={o.chunk_id} href={`/source/${n.id}?c=${o.chunk_id}`} className="row-hover -mx-2 flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px]"><span className="truncate">{o.heading.replace(/^[#§]+/, '').slice(0, 60)}</span><ExternalLink size={12} className="ml-auto shrink-0 text-mut" /></a>)}</div></>}
          {cites.length > 0 && <><div className="mb-1 mt-4 text-[12px] font-medium text-mut">Items filed from it</div>{cites.map((i) => <button key={i.id} className="row-hover -mx-2 block w-full rounded-lg px-2 py-1.5 text-left text-[13px]" onClick={() => pick({ kind: 'item', id: i.id })}>{i.title}</button>)}</>}
          {linkedU.length > 0 && <><div className="mb-1 mt-4 text-[12px] font-medium text-mut">Linked notes</div>{linkedU.map((x) => <button key={x.id} className="row-hover -mx-2 block w-full rounded-lg px-2 py-1.5 text-left text-[13px]" onClick={() => pick({ kind: 'note', id: x.id })}>{x.title}</button>)}</>}
          <a className="btn mt-4 w-full justify-center" href={`/source/${n.id}`}>Read the whole {n.kind}</a>
        </div>
      </aside>
    )
  }
  const it = f.items.find((x) => x.id === sel.id)
  if (!it) return null
  const src = f.notes.find((x) => x.id === it.source_id)
  return (
    <aside className="absolute right-3 top-3 z-10 w-[340px] overflow-auto rounded-[22px] border border-line2 bg-[#141416]/92 p-4 shadow-[0_24px_60px_rgba(0,0,0,.55),inset_0_1px_0_rgba(255,255,255,.05)] backdrop-blur-xl">
      <div className="flex items-start gap-2"><div><div className="text-[12px] capitalize text-mut">{it.tag === 'bug' ? 'Bug' : it.type}{it.state !== 'open' && ` · ${it.state.replace('_', ' ')}`}</div><div className="text-[16px] font-semibold leading-tight">{it.title}</div></div><button className="btn ghost sm ml-auto" onClick={close}><X size={14} /></button></div>
      {it.body && <p className="mt-2 text-[13px] leading-5 text-soft">{it.body}</p>}
      {it.answer && <p className="mt-2 text-[13px] text-accent">{it.answer}</p>}
      {it.quote && it.quote !== it.title && <blockquote className="mt-3 border-l-2 border-line2 pl-3 text-[12.5px] leading-5 text-mut">{it.quote}</blockquote>}
      {src && <button className="row-hover -mx-2 mt-3 flex w-[calc(100%+16px)] items-center gap-2 rounded-lg px-2 py-1.5 text-left text-[13px]" onClick={() => pick({ kind: 'note', id: src.id })}><FileText size={14} className="text-mut" /><span className="truncate">From {src.title}</span></button>}
      {it.source_id && <a className="mt-1 block text-[12px] text-mut hover:text-ink" href={`/source/${it.source_id}${it.chunk_id ? `?c=${it.chunk_id}` : ''}`}>Open the cited passage</a>}
      <div className="mt-4 flex flex-wrap gap-2">
        {it.type === 'step' && it.state !== 'done' && <button className="btn sm primary" onClick={async () => { await post('/api/tick', { id: it.id }); reload() }}>Mark done</button>}
        {it.type === 'step' && (it.state === 'open' || it.state === 'doing') && <button className="btn sm" onClick={async () => { await post('/api/doing', { id: it.id }); reload() }}>{it.state === 'doing' ? 'Stop' : 'Start'}</button>}
        {it.state === 'proposed' && <><button className="btn sm primary" onClick={async () => { await post('/api/review', { id: it.id, accept: true }); reload() }}>Accept</button><button className="btn sm" onClick={async () => { const reason = prompt('Why reject?') ?? ''; await post('/api/review', { id: it.id, accept: false, reason }); reload() }}>Reject</button></>}
      </div>
      {it.type === 'question' && it.state === 'open' && <form className="mt-4 flex gap-2" onSubmit={async (e) => { e.preventDefault(); const r = await post<Done>('/api/answer', { id: it.id, text: ans }); if (r.ok) { setAns(''); reload() } }}><input className="input" maxLength={320} placeholder="Your answer" value={ans} onChange={(e) => setAns(e.target.value)} /><button className="btn primary">Answer</button></form>}
    </aside>
  )
}

export default function MapPage() {
  const [sp, setSp] = useSearchParams()
  const nav = useNavigate()
  const thread = sp.get('thread')
  const { capture } = useShell()
  const { data: focus, reload } = useApi<Focus>(thread ? `/api/focus?thread=${encodeURIComponent(thread)}` : null)
  const [sel, setSel] = useState<Sel | null>(null)
  const [allItems, setAllItems] = useState(false)
  const [selThread, setSelThread] = useState<string | null | undefined>(undefined)
  if (selThread !== thread) {
    setSelThread(thread)
    const n = sp.get('note'), i = sp.get('item')
    setSel(n ? { kind: 'note', id: Number(n) } : i ? { kind: 'item', id: i } : null)
  }
  useEffect(() => { const h = () => reload(); window.addEventListener('tm:changed', h); return () => window.removeEventListener('tm:changed', h) }, [reload])

  const layout = useMemo(() => {
    const key = (thread ?? 'all') + allItems
    return focus?.thread?.name === thread ? { key, ...focusLayout(focus, sel, allItems) } : null
  }, [thread, focus, sel, allItems])

  const last = useRef<typeof layout>(null)
  if (layout) last.current = layout
  const L = layout ?? last.current
  const sizes = useRef(new Map<string, { width: number; height: number }>())
  const layoutKey = L?.key
  const onNodesChange = useCallback((changes: NodeChange[]) => { for (const c of changes) if (c.type === 'dimensions' && c.dimensions && layoutKey) sizes.current.set(`${layoutKey}|${c.id}`, c.dimensions) }, [layoutKey])
  const nodes = useMemo(() => L ? L.nodes.map((n) => ({ ...n, measured: sizes.current.get(`${L.key}|${n.id}`) })) : [], [L])

  if (!L) return <div className="grid h-full place-items-center text-mut">Loading the map</div>
  return (
    <div className="relative h-full min-h-[560px]">
      <div className="absolute left-4 top-4 z-10 flex items-center gap-2">
        <button className="btn glass" onClick={() => nav('/map')}><ArrowLeft size={14} />All threads</button>
        {thread && <button className="btn glass" onClick={() => nav(`/thread/${encodeURIComponent(thread)}`)}>Open page</button>}
        {thread && <button className="btn glass" onClick={() => capture(thread)}><Plus size={14} />Add to {thread}</button>}
        {thread && focus && (focus.items.length > 7 || focus.notes.length > 8) && <button className="btn glass" onClick={() => setAllItems(!allItems)}>{allItems ? 'Show fewer' : `Show all ${focus.items.length} items and ${focus.notes.length + focus.hidden_notes} notes`}</button>}
      </div>
      <ReactFlow key={L.key} nodes={nodes} edges={L.edges} onNodesChange={onNodesChange} nodeTypes={nodeTypes} fitView fitViewOptions={{ padding: 0.12, maxZoom: 1 }} minZoom={0.2} maxZoom={1.6}
        nodesConnectable={false} proOptions={{ hideAttribution: true }} colorMode="dark"
        onNodeClick={(_, n) => {
          if (n.type === 'linked') setSp({ thread: String(n.data.name) })
          else if (n.type === 'note') setSel({ kind: 'note', id: Number(n.data.id) })
          else if (n.type === 'item') setSel({ kind: 'item', id: String(n.data.id) })
        }} onPaneClick={() => setSel(null)}>
        <Background gap={26} size={1.4} color="#202025" />
        <Controls showInteractive={false} position="bottom-left" />
      </ReactFlow>
      {thread && focus && sel && <Panel f={focus} sel={sel} close={() => setSel(null)} reload={() => { reload(); }} pick={setSel} />}
      {thread && focus && !allItems && focus.notes.length > 8 && <div className="absolute bottom-4 right-4 z-10 rounded-full border border-line2 bg-[#141416]/90 px-4 py-2 text-[12px] text-mut backdrop-blur">{focus.notes.length - 8} more notes hidden</div>}
    </div>
  )
}
