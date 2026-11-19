import { useEffect, useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import { ArrowLeft } from 'lucide-react'
import { dayFmt, get, useApi } from '@/lib/api'
import Markdown from '@/components/Markdown'
import type { SourcePage, SourceRaw } from '@/lib/types'
import '@/next.css'

const KIND_NAME: Record<string, string> = { note: 'Note', artifact: 'Artifact', session: 'Conversation', commit: 'Commits', owner: 'Typed in the app' }

export default function Source() {
  const { id } = useParams()
  const [sp] = useSearchParams()
  const c = sp.get('c')
  const { data } = useApi<SourcePage>(`/api/source?id=${id}`)
  const [raw, setRaw] = useState<SourceRaw | null>(null)
  const [view, setView] = useState<'formatted' | 'passages'>('formatted')
  useEffect(() => { setRaw(null); get<SourceRaw>(`/api/source_raw?id=${id}`).then(setRaw) }, [id])
  useEffect(() => { const el = document.getElementById(`c${c}`); el?.scrollIntoView({ block: 'center' }) }, [data, view, c])
  if (!data?.source) return <div className="p-8 text-mut">Loading</div>
  const s = data.source
  const cited = data.chunks.find((x) => String(x.id) === c)
  const canFormat = raw && raw.text && (raw.kind === 'note' || raw.kind === 'artifact')
  const kindName = KIND_NAME[s.kind] ?? s.kind

  return (
    <div className="nx-page flex flex-col gap-4">
      <div>
        <Link to={s.thread ? `/thread/${encodeURIComponent(s.thread)}` : '/'} className="inline-flex items-center gap-1.5 text-[13px] text-mut hover:text-ink"><ArrowLeft size={14} />{s.thread ?? 'Today'}</Link>
        <div className="mt-3 flex flex-wrap items-center gap-2"><span className="chip">{kindName}</span><span className="chip">{dayFmt(s.first_ts)}{s.last_ts && s.last_ts.slice(0, 10) !== s.first_ts?.slice(0, 10) ? ` to ${dayFmt(s.last_ts)}` : ''}</span><span className="chip">{s.nchunks} passages</span></div>
        <h2 className="mt-2 max-w-[28ch] text-[30px] font-semibold leading-[1.15] tracking-tight">{s.title}</h2>
        {data.items.length > 0 && <div className="mt-3 flex flex-wrap gap-1.5">{data.items.slice(0, 5).map((i) => <Link key={i.id} to={`/thread/${encodeURIComponent(i.thread)}`} className="chip max-w-[320px] truncate hover:text-ink">{i.title}</Link>)}{data.items.length > 5 && <span className="chip">{data.items.length - 5} more filed from this</span>}</div>}
      </div>
      {canFormat && <div className="flex gap-1 self-start rounded-full border border-line2 bg-white/[.03] p-1">{(['formatted', 'passages'] as const).map((v) => <button key={v} onClick={() => setView(v)} className={`h-8 rounded-full px-4 text-[13px] transition ${view === v ? 'bg-white/10 font-medium text-ink' : 'text-mut hover:text-ink'}`}>{v === 'formatted' ? 'Formatted' : 'Passages'}</button>)}</div>}
      {cited && <div className="rounded-2xl border border-accent/40 bg-accent/5 p-5"><div className="mb-1 text-[12px] font-medium text-accent">The passage that was cited</div><div className="whitespace-pre-wrap text-[13.5px] text-soft">{cited.text.slice(0, 900)}</div></div>}

      {canFormat && view === 'formatted' && raw?.kind === 'note' && <div className="card p-6"><Markdown text={raw.text} /></div>}
      {canFormat && view === 'formatted' && raw?.kind === 'artifact' && <iframe title={s.title} sandbox="" srcDoc={raw.text} className="h-[78vh] w-full rounded-2xl border border-line bg-white" />}
      {(!canFormat || view === 'passages') && (
        <div className="card overflow-hidden">
          {data.chunks.map((x) => (
            <div id={`c${x.id}`} key={x.id} className={`border-t border-line px-5 py-3 first:border-0 ${String(x.id) === c ? 'bg-open/10' : ''}`}>
              <div className="mb-1 text-[12px] text-mut">{x.role === 'user' ? 'You' : x.role === 'assistant' ? 'Claude' : ''} {x.locator.replace(/^[#§]/, '')}</div>
              <div className="whitespace-pre-wrap text-[13.5px] leading-6 text-soft">{x.text}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
