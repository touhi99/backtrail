import { useEffect, useMemo, useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { CircleHelp, ListTodo, Scale, Sparkles } from 'lucide-react'
import { dayFmt, get, STALE_LABEL, staleOf } from '@/lib/api'
import { useShell } from '@/components/Shell'
import type { LucideIcon } from 'lucide-react'
import type { Item } from '@/lib/types'
import ItemActions, { Who } from '@/components/ItemActions'
import Paged from '@/components/Paged'
import '@/next.css'

const OPT = {
  type: [['', 'All types'], ['decision', 'Decisions'], ['step', 'Steps'], ['question', 'Questions'], ['turn', 'Turns']],
  state: [['', 'Any state'], ['open', 'To do'], ['doing', 'In progress'], ['done', 'Done'], ['looks_done', 'Built, check it'], ['settled', 'Settled'], ['answered', 'Answered'], ['proposed', 'Suggested'], ['rejected', 'Rejected'], ['parked', 'Not valid now']],
  kind: [['', 'Any kind'], ['idea', 'Idea'], ['plan', 'Plan'], ['build', 'Build'], ['study', 'Study'], ['discussion', 'Discussion']],
}
const TYPE: Record<string, { icon: LucideIcon; c: string }> = {
  question: { icon: CircleHelp, c: '#fbbf24' }, step: { icon: ListTodo, c: '#60a5fa' }, decision: { icon: Scale, c: '#c084fc' }, turn: { icon: Sparkles, c: '#8b8b95' },
}
const HELP: Record<string, string> = {
  all: 'Every decision, step and question filed across threads. The tag shows who filed it, You or Claude. Filter to narrow it; open a title for its thread, or act on a row at the right.',
  decision: 'Choices made in sessions. A suggested decision is accepted to keep it or rejected to drop it. A kept decision stays standing until marked replaced.',
  step: 'Todos and bugs. Open the thread to start, tick or confirm them.',
  question: 'Questions that wait for an answer. Answer them on Today or on the thread page.',
}
const STATE_LABEL: Record<string, string> = { proposed: 'suggested', open: 'to do', doing: 'in progress', looks_done: 'built, check it', parked: 'not valid now' }
const CLOSED = ['done', 'answered', 'settled', 'rejected', 'parked']
const STATE_C: Record<string, string> = { open: '#fbbf24', doing: '#60a5fa', done: '#34d399', looks_done: '#34d399', settled: '#c084fc', answered: '#34d399', proposed: '#8b8b95', rejected: '#fb7185', parked: '#8b8b95' }

export default function Items() {
  const { today } = useShell()
  const [sp, setSp] = useSearchParams()
  const f = { type: sp.get('type') ?? '', state: sp.get('state') ?? '', kind: sp.get('kind') ?? '', thread: sp.get('thread') ?? '' }
  const [all, setRows] = useState<Item[] | null>(null)
  const showClosed = sp.get('closed') === '1' || f.state !== ''
  const nClosed = (all ?? []).filter((i) => CLOSED.includes(i.state)).length
  const rows = all && (showClosed ? all : all.filter((i) => !CLOSED.includes(i.state)))
  const [tick, setTick] = useState(0)
  const reloadAll = () => setTick((t) => t + 1)
  useEffect(() => { get<Item[]>(`/api/items?type=${f.type}&state=${f.state}&kind=${f.kind}&thread=${encodeURIComponent(f.thread)}`).then(setRows) }, [f.type, f.state, f.kind, f.thread, tick])
  const set = (k: string, v: string) => { const n = new URLSearchParams(sp); if (v) n.set(k, v); else n.delete(k); setSp(n) }
  const sel = (k: keyof typeof OPT) => <select className="input !w-auto !rounded-full !px-4" value={f[k]} onChange={(e) => set(k, e.target.value)}>{OPT[k].map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select>
  const mix = useMemo(() => { const m: Record<string, number> = {}; (rows ?? []).forEach((i) => (m[i.state] = (m[i.state] ?? 0) + 1)); return Object.entries(m).sort((a, b) => b[1] - a[1]) }, [rows])
  return (
    <div className="nx-page">
      <div className="nx-in flex items-end justify-between"><div><div className="text-[13px] text-mut">Everything filed</div><h2 className="text-[30px] font-semibold leading-tight tracking-tight" title={HELP[f.type] ?? HELP.all}>Items</h2></div>
        <div className="text-right"><div className="nx-big">{rows?.length ?? 0}</div></div></div>
      <div className="nx-in mt-4 flex flex-wrap items-center gap-2" style={{ '--d': '40ms' }}>{sel('type')}{sel('state')}{sel('kind')}
        <select className="input !w-auto !rounded-full !px-4" value={f.thread} onChange={(e) => set('thread', e.target.value)}><option value="">All threads</option>{(today?.threads ?? []).map((t) => <option key={t.name}>{t.name}</option>)}</select></div>
      {mix.length > 0 && (
        <div className="nx-in mt-4" style={{ '--d': '80ms' }}>
          <div className="flex h-2 gap-[3px] overflow-hidden rounded-full">{mix.map(([s, n]) => <span key={s} title={`${s.replace('_', ' ')}: ${n}`} className="rounded-full" style={{ flex: n, background: STATE_C[s] ?? '#52525b' }} />)}</div>
          <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[12px] text-mut">{mix.map(([s, n]) => <span key={s} className="flex items-center gap-1.5"><span className="size-2 rounded-full" style={{ background: STATE_C[s] ?? '#52525b' }} />{s.replace('_', ' ')} <span className="tabular-nums">{n}</span></span>)}</div>
        </div>
      )}
      <section className="nx-panel nx-in mt-4 overflow-hidden" style={{ '--d': '120ms' }}>
        <Paged resetKey={sp.toString()} items={rows ?? []} size={15}>{(pageRows) => pageRows.map((i) => {
          const T = TYPE[i.tag === 'bug' ? 'step' : i.type] ?? TYPE.turn, Icon = T.icon, st = staleOf(i)
          return (
            <div key={i.id} className="nx-row" style={{ gridTemplateColumns: '36px minmax(0,1fr) auto auto', cursor: 'default', opacity: (i.stale ?? '').startsWith('superseded') ? 0.55 : 1 }}>
              <span className="nx-glyph size-9" style={{ background: T.c + '1f', color: T.c }}><Icon size={16} /></span>
              <div className="min-w-0">
                <Link to={`/thread/${encodeURIComponent(i.thread)}?item=${i.id}`} title={[i.body, i.answer].filter(Boolean).join(" · ")} className="block truncate text-[14px] font-medium hover:underline">{i.title}</Link>
              </div>
              <div className="flex items-center gap-2 text-[12px] text-mut">
                {st && <span className="nx-chip" style={{ background: '#fbbf2418', color: '#fbbf24' }}>{STALE_LABEL[st.kind]}</span>}
                <Who i={i} /><span className="chip"><span className="size-1.5 rounded-full" style={{ background: STATE_C[i.state] ?? '#52525b' }} />{STATE_LABEL[i.state] ?? i.state}</span>
                <span className="hidden w-[120px] truncate text-right lg:block">{i.thread}</span>
                <span className="w-[52px] text-right tabular-nums">{dayFmt(i.source_ts ?? i.created)}</span>
              </div>
              <div className="flex min-w-[150px] justify-end"><ItemActions i={i} done={reloadAll} /></div>
            </div>
          )
        })}</Paged>
        {all && !showClosed && nClosed > 0 && <button type="button" className="w-full px-5 py-3 text-left text-[13px] text-mut hover:text-ink" onClick={() => set('closed', '1')}>Show closed ({nClosed}): done, answered, settled, rejected, not valid now</button>}
        {all && sp.get('closed') === '1' && f.state === '' && <button type="button" className="w-full px-5 py-3 text-left text-[13px] text-mut hover:text-ink" onClick={() => set('closed', '')}>Hide closed</button>}
        {rows && rows.length === 0 && <div className="px-5 py-6 text-[13.5px] text-mut">No items match.</div>}
      </section>
    </div>
  )
}
