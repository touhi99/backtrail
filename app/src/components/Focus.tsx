import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { Check, CircleHelp, FileText, MessageCircleQuestion, Pin, Plus } from 'lucide-react'
import { dayFmt, post } from '@/lib/api'

/** Items older than this leave the focus deck and wait in the archive. */
export const WEEK = 7 * 864e5
export const DECK = 3
export const ts = (i: any) => Date.parse(i.source_ts ?? i.created ?? '') || 0

const LATER_KEY = 'bt-later'
const readLater = (): string[] => { try { const v = JSON.parse(localStorage.getItem(LATER_KEY) ?? '[]'); return Array.isArray(v) ? v.map(String) : [] } catch { return [] } }
/** Ids the viewer put off with Later; kept in this browser only. */
export function useLater() {
  const [later, setLater] = useState<string[]>(readLater)
  const add = (id: string) => { const n = [...later.filter((x) => x !== id), id].slice(-500); setLater(n); try { localStorage.setItem(LATER_KEY, JSON.stringify(n)) } catch { /* private window */ } }
  return { later, add }
}

type Kind = 'done' | 'question' | 'step' | 'decision' | 'idea'
const KIND: Record<Kind, { label: string; color: string; icon: any; yes: string; no: string }> = {
  done: { label: 'Done?', color: '#fbbf24', icon: Check, yes: 'Yes, done', no: 'Not yet' },
  question: { label: 'Claude asks', color: '#c084fc', icon: MessageCircleQuestion, yes: 'Answer', no: '' },
  step: { label: 'New step', color: '#60a5fa', icon: Plus, yes: 'Add', no: "Don't add" },
  decision: { label: 'Decision', color: '#d4d4d8', icon: Pin, yes: 'Keep', no: 'Wrong' },
  idea: { label: 'Open question', color: '#c084fc', icon: CircleHelp, yes: 'Keep', no: 'Drop' },
}
const kindOf = (i: any): Kind => (i.state === 'looks_done' ? 'done' : i.state === 'open' && i.type === 'question' ? 'question' : i.type === 'step' ? 'step' : i.type === 'question' ? 'idea' : 'decision')

function FocusCard({ i, pos, total, act, later, project }: { i: any; pos: number; total: number; act: (url: string, body: any) => Promise<void>; later: () => void; project?: { name: string; color: string } }) {
  const k = kindOf(i), K = KIND[k], Icon = K.icon
  const [ans, setAns] = useState('')
  const long = i.title.length > 48
  const yes = () => k === 'done' ? act('/api/tick', { id: i.id }) : act('/api/review', { id: i.id, accept: true })
  const no = () => k === 'done' ? act('/api/notyet', { id: i.id }) : act('/api/review', { id: i.id, accept: false, reason: 'Marked wrong in the focus card' })
  const when = k === 'done' ? i.updated : i.source_ts ?? i.created
  return (
    <article id={`item-${i.id}`} className="nx-in relative flex min-h-[300px] flex-col gap-4 rounded-[22px] border px-7 py-6"
      style={{ background: 'linear-gradient(180deg,#17171b,#111114)', borderColor: K.color + '40', boxShadow: `0 30px 80px -34px ${K.color}66` }}>
      <div className="flex flex-wrap items-center gap-2.5">
        <span className="inline-flex h-[30px] items-center gap-2 rounded-full px-3 text-[13px] font-medium" style={{ background: K.color + '1f', color: K.color }}><Icon size={15} strokeWidth={2.4} />{K.label}</span>
        {project && <Link to={`/thread/${encodeURIComponent(project.name)}`} className="inline-flex h-[30px] items-center gap-2 rounded-full bg-[#1c1c21] px-3 text-[13px] text-soft hover:text-ink"><span className="size-[7px] rounded-full" style={{ background: project.color }} />{project.name}</Link>}
        <div className="ml-auto flex gap-1.5" aria-label={`${pos + 1} of ${total}`}>
          {Array.from({ length: total }, (_, n) => <span key={n} className="h-1.5 rounded-full transition-all" style={{ width: n === pos ? 22 : 6, background: n < pos ? '#34d399' : n === pos ? '#f4f4f5' : '#3f3f46' }} />)}
        </div>
      </div>
      <h2 className={`max-w-[26ch] font-semibold leading-[1.18] tracking-tight line-clamp-3 ${long ? 'text-[24px]' : 'text-[30px]'}`} title={i.title}>{i.title}</h2>
      {k === 'done' && i.answer && <p className="line-clamp-2 max-w-[60ch] text-[14px] text-soft">{i.answer}</p>}
      {k === 'question' && i.body && <p className="line-clamp-2 max-w-[60ch] text-[14px] text-soft">{i.body}</p>}
      <div className="flex flex-wrap gap-2">
        {when && <span className="inline-flex h-7 items-center rounded-[9px] bg-[#1c1c21] px-2.5 text-[12.5px] text-soft">{dayFmt(when)}</span>}
        {i.source_id && <Link to={`/source/${i.source_id}${i.chunk_id ? `?c=${i.chunk_id}` : ''}`} className="inline-flex h-7 max-w-[280px] items-center gap-1.5 truncate rounded-[9px] bg-[#1c1c21] px-2.5 text-[12.5px] text-soft hover:text-ink" title={i.quote || i.source_title}><FileText size={12} className="shrink-0" /><span className="truncate">{i.source_title || 'Source'}</span></Link>}
      </div>
      {k === 'question' ? (
        <form className="mt-auto flex flex-wrap gap-2.5" onSubmit={async (e) => { e.preventDefault(); if (ans.trim()) await act('/api/answer', { id: i.id, text: ans }) }}>
          <input className="nx-input" maxLength={320} placeholder="Type your answer" value={ans} onChange={(e) => setAns(e.target.value)} aria-label="Answer" />
          <button className="btn primary !h-11 !px-5 !text-[14px]" type="submit">Answer</button>
          <button className="btn ghost !h-11" type="button" onClick={later} title="Skipped items move to the archive after a week">Later</button>
        </form>
      ) : (
        <div className="mt-auto flex flex-wrap gap-2.5">
          <button className="btn primary !h-11 !rounded-xl !px-5 !text-[14px]" onClick={yes}>{K.yes}</button>
          <button className="btn !h-11 !rounded-xl !px-5 !text-[14px]" onClick={no}>{K.no}</button>
          <button className="btn ghost !h-11 ml-auto" onClick={later} title="Skipped items move to the archive after a week">Later</button>
        </div>
      )}
    </article>
  )
}

/**
 * Up to three items at a time from `queue`, one in focus, with the rest peeking underneath. The batch stays fixed
 * until it is cleared; then "Show three more" takes the next batch. `focus` puts that item first.
 */
export function FocusDeck({ queue, refresh, resetKey, focus, projectOf }: { queue: any[]; refresh: () => void; resetKey: string; focus?: string | null; projectOf?: (i: any) => { name: string; color: string } | undefined }) {
  const { add } = useLater()
  const [batch, setBatch] = useState<string[] | null>(null)
  const [pos, setPos] = useState(0)
  const [gone, setGone] = useState<string[]>([])
  const live = queue.filter((q) => !gone.includes(q.id))
  useEffect(() => { setBatch(null); setPos(0); setGone([]) }, [resetKey])
  useEffect(() => { if (batch === null && queue.length) setBatch(live.slice(0, DECK).map((i) => i.id)) }, [batch, queue])
  useEffect(() => { if (focus && live.some((q) => q.id === focus)) { setBatch([focus, ...live.filter((q) => q.id !== focus).slice(0, DECK - 1).map((q) => q.id)]); setPos(0) } }, [focus, queue.length > 0])
  const ids = batch ?? []
  const card = ids.slice(pos).map((id) => live.find((q) => q.id === id)).find(Boolean)
  const cardPos = card ? ids.indexOf(card.id) : ids.length
  const after = card ? ids.slice(cardPos + 1).filter((id) => live.some((q) => q.id === id)).length : 0
  const more = live.some((q) => !ids.includes(q.id))
  const next = (id: string) => { setGone((g) => [...g, id]); setPos(ids.indexOf(id) + 1) }
  const act = async (url: string, body: any) => { const r = await post(url, body); if (r?.ok !== false) next(body.id); refresh() }
  return (
    <section className="relative min-w-0 pb-[22px]" style={{ flex: '999 1 560px' }}>
      {card ? (<>
        {after > 1 && <div className="absolute bottom-0 left-9 right-9 h-16 rounded-[22px] border border-[#1c1c21] bg-[#101013]" />}
        {after > 0 && <div className="absolute bottom-[11px] left-[18px] right-[18px] h-16 rounded-[22px] border border-[#222228] bg-[#141418]" />}
        <FocusCard key={card.id} i={card} pos={cardPos} total={ids.length} act={act} later={() => { add(card.id); next(card.id) }} project={projectOf?.(card)} />
      </>) : (
        <article className="nx-in grid min-h-[300px] place-items-center rounded-[22px] border border-[#1f3b31] text-center" style={{ background: 'linear-gradient(180deg,#0f1c16,#0c130f)', boxShadow: '0 30px 80px -34px rgba(52,211,153,.4)' }}>
          <div>
            <div className="mx-auto mb-3.5 grid size-14 place-items-center rounded-full bg-[#34d39926] text-accent"><Check size={26} strokeWidth={2.6} /></div>
            <div className="text-[24px] font-semibold">All caught up</div>
            {more && <button className="btn ghost mt-3" onClick={() => { setBatch(live.slice(0, DECK).map((i) => i.id)); setPos(0) }}>Show three more</button>}
          </div>
        </article>
      )}
    </section>
  )
}
