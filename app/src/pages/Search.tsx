import { useEffect, useState } from 'react'
import { Link, useNavigate, useSearchParams } from 'react-router-dom'
import { Search as SearchIcon } from 'lucide-react'
import { dayFmt, get } from '@/lib/api'
import { useShell } from '@/components/Shell'
import Paged from '@/components/Paged'
import '@/next.css'

const KIND = [['', 'Any source'], ['session', 'Conversations'], ['note', 'Notes'], ['artifact', 'Artifacts'], ['commit', 'Commits']]
const STATE = [['', 'Any item state'], ['open', 'Open'], ['done', 'Done'], ['settled', 'Settled'], ['answered', 'Answered'], ['proposed', 'Suggested'], ['rejected', 'Rejected']]
const STATE_C: Record<string, string> = { open: '#fbbf24', doing: '#60a5fa', done: '#34d399', looks_done: '#34d399', settled: '#c084fc', answered: '#34d399', proposed: '#8b8b95', rejected: '#fb7185' }
const KEYS = ['thread', 'kind', 'state', 'from', 'to', 'mode'] as const

export default function Search() {
  const { today } = useShell()
  const [sp, setSp] = useSearchParams()
  const nav = useNavigate()
  const q = sp.get('q') ?? ''
  const [text, setText] = useState(q)
  const [r, setR] = useState<any>(null)
  const f = Object.fromEntries(KEYS.map((k) => [k, sp.get(k) ?? ''])) as Record<(typeof KEYS)[number], string>
  const qs = sp.toString()
  useEffect(() => {
    setText(q)
    if (!q) { setR(null); return }
    const p = new URLSearchParams(); p.set('q', q); KEYS.forEach((k) => f[k] && p.set(k, f[k]))
    get(`/api/search?${p}`).then(setR)
  }, [qs]) // eslint-disable-line react-hooks/exhaustive-deps
  const set = (k: string, v: string) => { const n = new URLSearchParams(sp); v ? n.set(k, v) : n.delete(k); setSp(n) }
  const submit = () => { const n = new URLSearchParams(sp); n.set('q', text); setSp(n) }
  const active = KEYS.some((k) => k !== 'mode' && f[k])
  return (
    <div className="nx-page flex flex-col gap-4">
      <form onSubmit={(e) => { e.preventDefault(); submit() }} className="relative">
        <SearchIcon size={16} className="absolute left-4 top-1/2 -translate-y-1/2 text-mut" /><input autoFocus className="input !h-14 !rounded-2xl !pl-12 !text-[16px]" placeholder="Search notes, artifacts, conversations and commits" value={text} onChange={(e) => setText(e.target.value)} />
      </form>
      <div className="flex flex-wrap items-center gap-2">
        <select className="input !w-auto !rounded-full !px-4" value={f.thread} onChange={(e) => set('thread', e.target.value)}><option value="">All projects</option>{(today?.threads ?? []).map((t: any) => <option key={t.name}>{t.name}</option>)}</select>
        <select className="input !w-auto !rounded-full !px-4" value={f.kind} onChange={(e) => set('kind', e.target.value)}>{KIND.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select>
        <select className="input !w-auto !rounded-full !px-4" value={f.state} onChange={(e) => set('state', e.target.value)}>{STATE.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select>
        <label className="flex items-center gap-1.5 text-[12px] text-mut">From<input type="date" className="input !h-9 !w-auto !rounded-full !px-3" value={f.from} onChange={(e) => set('from', e.target.value)} /></label>
        <label className="flex items-center gap-1.5 text-[12px] text-mut">to<input type="date" className="input !h-9 !w-auto !rounded-full !px-3" value={f.to} onChange={(e) => set('to', e.target.value)} /></label>
        <label className="flex items-center gap-1.5 text-[12px] text-mut"><input type="checkbox" checked={f.mode === 'text'} onChange={(e) => set('mode', e.target.checked ? 'text' : '')} />Keywords only</label>
        {active && <button className="text-[12px] text-mut underline" onClick={() => { const n = new URLSearchParams(); if (q) n.set('q', q); setSp(n) }}>Clear filters</button>}
      </div>
      {r && <div className="text-[12px] text-mut">{r.mode === 'hybrid' ? 'Meaning and keyword match' : 'Keyword match'}{r.note ? ` · ${r.note}` : ''}</div>}
      {r && r.items.length > 0 && <section className="nx-panel nx-in overflow-hidden"><div className="px-5 pb-2 pt-4 text-[15px] font-semibold">Filed items</div>{r.items.map((i: any) => <Link key={i.id} to={`/thread/${encodeURIComponent(i.thread)}`} className="nx-row !block"><div className="flex items-center gap-2 text-[12px] text-mut"><span className="chip !h-[20px] !text-[11px]">{i.type}</span>{i.thread}<span className="ml-auto flex items-center gap-1.5"><span className="size-2 rounded-full" style={{ background: STATE_C[i.state] ?? '#52525b' }} />{i.state.replace('_', ' ')}</span></div><div className="text-[14px] font-medium">{i.title}</div></Link>)}</section>}
      {r && <section className="nx-panel nx-in overflow-hidden" style={{ ['--d' as any]: '60ms' }}><div className="px-5 pb-2 pt-4 text-[15px] font-semibold">Passages <span className="text-[12px] font-normal text-mut">{r.results.length} best matches</span></div>
        <Paged resetKey={qs} items={r.results} size={10}>{(pageRows: any[]) => pageRows.map((x: any) => <Link key={x.chunk_id} to={`/source/${x.source_id}?c=${x.chunk_id}`} className="nx-row !block"><div className="flex items-center gap-2 text-[12px] text-mut"><span className="chip !h-[20px] !text-[11px] capitalize">{x.kind}</span>{x.thread && <span>{x.thread}</span>}<span className="ml-auto">{dayFmt(x.last_ts)}</span></div><div className="text-[14px] font-medium">{x.title}</div><div className="mt-0.5 line-clamp-2 text-[13px] text-soft [&_mark]:rounded [&_mark]:bg-open/30 [&_mark]:px-0.5 [&_mark]:text-ink" dangerouslySetInnerHTML={{ __html: x.snip }} />
          {(x.why?.length > 0 || x.items?.length > 0) && <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11.5px] text-mut">{x.why.map((w: string) => <span key={w} className="chip !h-[20px] !text-[11px]">{w}</span>)}{x.items.slice(0, 2).map((i: any) => <span key={i.id} className="flex items-center gap-1.5"><span className="size-1.5 rounded-full" style={{ background: i.dead ? '#fb7185' : STATE_C[i.state] ?? '#52525b' }} />{i.type}: {i.title.length > 60 ? i.title.slice(0, 60) + '…' : i.title}</span>)}</div>}
        </Link>)}</Paged>
        {r.results.length === 0 && <div className="px-5 pb-4 text-[13px] text-mut">No match.</div>}</section>}
      {!q && <div className="text-[13.5px] text-mut">Type a word or a question.</div>}
    </div>
  )
}
