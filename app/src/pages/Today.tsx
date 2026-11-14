import { useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { MessageCircleQuestion } from 'lucide-react'
import { agoFmt, dayFmt, post, staleOf } from '@/lib/api'
import { useShell } from '@/components/Shell'
import { STATE_HEX, dayCounts, nextStep, useDetails } from '@/lib/next'
import { Ring } from '@/components/Viz'
import { FocusDeck, useLater, ts, WEEK } from '@/components/Focus'
import { TrailCard } from '@/pages/Trail'
import '@/next.css'

const DAYS = 8

export default function Home() {
  const { today, reload } = useShell()
  const { later } = useLater()
  const [allQ, setAllQ] = useState(false)
  const [tab, setTab] = useState<'decide' | 'todo' | 'asks'>('decide')
  const [openQ, setOpenQ] = useState<string | null>(null)
  const [ans, setAns] = useState('')
  const threads: any[] = today?.threads ?? []
  const moving = threads.filter((t) => t.state === 'moving')
  const waiting: any[] = today?.waiting ?? []
  const details = useDetails(threads.map((t) => t.name))
  const now = Date.now()

  const queue = useMemo(() => {
    const looks = (today?.looks ?? []).map((l: any) => ({ ...l, type: 'step', state: 'looks_done' }))
    const asks = waiting.filter((q) => now - ts(q) < WEEK).map((q) => ({ ...q, type: 'question', state: 'open' }))
    const rank: Record<string, number> = { step: 0, question: 1, decision: 2 }
    const sugg = (today?.suggested ?? []).filter((s: any) => !staleOf(s) && now - ts(s) < WEEK).map((s: any) => ({ ...s, state: 'proposed' }))
      .sort((a: any, b: any) => (rank[a.type] ?? 3) - (rank[b.type] ?? 3) || ts(b) - ts(a))
    return [...looks, ...asks, ...sugg].filter((i) => !later.includes(i.id))
  }, [today, later])

  const week = useMemo(() => {
    const sessions = new Map<number, any>()
    let steps = 0, decisions = 0
    Object.values(details).forEach((d: any) => {
      ;(d?.sources ?? []).forEach((s: any) => { if (s.kind === 'session') sessions.set(s.id, s) })
      ;(d?.items ?? []).forEach((i: any) => { if (i.type === 'step' && i.state === 'done' && i.updated && now - Date.parse(i.updated) < WEEK) steps++; if (i.type === 'decision' && i.state !== 'rejected' && now - ts(i) < WEEK) decisions++ })
    })
    const days = dayCounts({ sources: Array.from(sessions.values()) }, DAYS)
    return { steps, decisions, days, total: days.reduce((a, b) => a + b, 0) }
  }, [details])

  const todo: any[] = today?.todo ?? []
  const asks: any[] = today?.asks ?? []
  const buckets = [
    { id: 'decide' as const, n: queue.length, label: 'Waiting for your decision', hint: 'Claude did or proposed it, you confirm' },
    { id: 'todo' as const, n: todo.length, label: 'To do', hint: 'Agreed and noted, not built yet' },
    { id: 'asks' as const, n: asks.length, label: 'Waiting on Claude', hint: 'You asked, no answer yet' },
  ].filter((b) => b.id !== 'asks' || b.n > 0)
  const list = tab === 'todo' ? todo : asks
  const color = (name: string) => STATE_HEX[threads.find((t) => t.name === name)?.state ?? 'moving'] ?? '#8b8b95'
  const when = new Date().toLocaleDateString('en-GB', { weekday: 'long', day: 'numeric', month: 'long' })
  const mx = Math.max(1, ...week.days)
  const submit = async (e: React.FormEvent, id: string) => { e.preventDefault(); const r = await post('/api/answer', { id, text: ans }); if (r.ok) { setAns(''); setOpenQ(null); reload() } }

  return (
    <div className="nx-page">
      <div className="pointer-events-none absolute inset-x-0 top-0 h-[420px]" style={{ background: 'radial-gradient(900px 420px at 30% -80px, rgba(52,211,153,.09), transparent 70%)' }} />
      <div className="relative mx-auto flex max-w-[1180px] flex-col gap-7">
        <header className="nx-in">
          <div className="text-[13px] text-mut">{when}</div>
          <h2 className="text-[28px] font-semibold leading-tight tracking-tight">Home</h2>
        </header>

        <TrailCard />

        <div className="grid grid-cols-2 gap-3 md:grid-cols-3" role="tablist">
          {buckets.map((b) => (
            <button key={b.id} role="tab" aria-selected={tab === b.id} onClick={() => setTab(b.id)} title={b.hint}
              className={`nx-panel flex flex-col gap-1 !rounded-2xl p-4 text-left transition ${tab === b.id ? '!border-[#34d399]' : 'hover:border-[#34343c]'}`}>
              <span className="text-[28px] font-semibold leading-none tabular-nums">{b.n}</span>
              <span className="text-[14px] font-medium">{b.label}</span>
              <span className="text-[12px] text-mut">{b.hint}</span>
            </button>
          ))}
        </div>

        <div className="flex flex-wrap items-stretch gap-5">
          {tab === 'decide' && <FocusDeck queue={queue} refresh={reload} resetKey="home" projectOf={(i) => i.thread ? { name: i.thread, color: color(i.thread) } : undefined} />}
          {tab !== 'decide' && (
            <section className="nx-panel nx-in min-w-0 overflow-hidden !rounded-2xl" style={{ flex: '999 1 560px' }}>
              {list.length === 0 && <div className="px-5 py-6 text-[14px] text-mut">Nothing here.</div>}
              {list.map((x) => (
                <Link key={x.id} to={`/thread/${encodeURIComponent(x.thread)}?item=${encodeURIComponent(x.id)}`} className="row-hover flex items-center gap-3 border-t border-line px-5 py-3 first:border-t-0">
                  <span className="size-2 shrink-0 rounded-full" style={{ background: color(x.thread) }} />
                  <span className="min-w-0 flex-1 truncate text-[14px]">{x.title}</span>
                  <span className="shrink-0 text-[12px] text-mut">{x.thread} · {agoFmt(x.created)}</span>
                </Link>
              ))}
              {tab === 'asks' && list.length > 0 && <div className="border-t border-line px-5 py-3 text-[12.5px] text-mut">Typed in the app. Claude answers at the start of its next session; nothing to do here.</div>}
            </section>
          )}
          <aside className="nx-panel nx-in flex flex-col gap-4 !rounded-2xl p-5" style={{ flex: '1 1 280px' }}>
            <div className="text-[13px] text-mut">This week</div>
            <div className="flex items-baseline gap-2"><span className="text-[44px] font-semibold leading-none tracking-tight tabular-nums">{week.steps}</span><span className="text-[15px] text-mut">steps finished</span></div>
            <div className="flex h-16 items-end gap-2" role="img" aria-label={`Sessions per day over the last ${DAYS} days: ${week.days.join(', ')}`}>
              {week.days.map((v, i) => <span key={i} className="flex-1 rounded-[5px]" title={`${v} sessions`} style={{ height: Math.max(4, (v / mx) * 64), background: i === DAYS - 1 ? '#34d399' : '#2a3a33' }} />)}
            </div>
            <div className="-mt-2 flex justify-between text-[12px] text-mut"><span>{DAYS} days ago</span><span>{week.total} sessions</span><span>today</span></div>
            <div className="grid grid-cols-2 gap-2.5">
              <div className="rounded-xl bg-[#18181c] p-3"><div className="text-[20px] font-semibold tabular-nums">{moving.length}</div><div className="text-[12.5px] text-mut">threads moving</div></div>
              <div className="rounded-xl bg-[#18181c] p-3"><div className="text-[20px] font-semibold tabular-nums">{week.decisions}</div><div className="text-[12.5px] text-mut">decisions noted</div></div>
            </div>
          </aside>
        </div>

        <section className="nx-in">
          <h3 className="mb-3 text-[17px] font-semibold">Moving</h3>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-3">
            {moving.map((t) => {
              const c = STATE_HEX[t.state], tot = t.done_s + t.open_s, nx = nextStep(details[t.name])
              const act = dayCounts(details[t.name], 14), amx = Math.max(1, ...act)
              return (
                <Link key={t.name} to={`/thread/${encodeURIComponent(t.name)}`} className="nx-panel flex flex-col gap-3 !rounded-2xl p-4 transition hover:-translate-y-px hover:border-[#34343c]">
                  <div className="flex items-center gap-3">
                    <span className="nx-glyph size-9 text-[15px] font-semibold" style={{ background: c + '1f', color: c }}>{t.name[0]}</span>
                    <span className="min-w-0 flex-1 truncate text-[15.5px] font-semibold">{t.name}</span>
                    <Ring value={tot ? t.done_s / tot : 0} color="#34d399" size={46} stroke={5}><span className="text-[10px] font-semibold tabular-nums text-soft">{t.done_s}/{tot}</span></Ring>
                  </div>
                  {nx && <div className="line-clamp-2 text-[13px] text-mut">Next: <span className="text-ink">{nx}</span></div>}
                  <div className="mt-auto flex h-5 items-end gap-[3px]" title="Sessions and notes in the last 14 days">
                    {act.map((v, i) => <span key={i} className="flex-1 rounded-full" style={{ height: 4 + (v / amx) * 16, background: v ? c : '#26262b', opacity: v ? 0.45 + 0.55 * (v / amx) : 1 }} />)}
                  </div>
                </Link>
              )
            })}
          </div>
        </section>

        <nav className="nx-in flex flex-wrap gap-2">
          <button className={`btn ${allQ ? '' : 'ghost'} !border-[#26262c]`} aria-pressed={allQ} onClick={() => setAllQ(!allQ)}>All questions</button>
          <Link className="btn ghost !border-[#26262c]" to="/items?state=proposed">All suggestions</Link>
        </nav>

        {allQ && (
          <section className="nx-panel nx-in overflow-hidden">
            {waiting.length === 0 && <div className="px-5 py-4 text-[13px] text-mut">No questions from Claude.</div>}
            {waiting.map((x) => {
              const isOpen = openQ === x.id
              return (
                <div key={x.id} className="border-t border-line first:border-t-0">
                  <button type="button" onClick={() => { setOpenQ(isOpen ? null : x.id); setAns('') }} className="row-hover flex w-full items-center gap-3 px-5 py-3 text-left">
                    <MessageCircleQuestion size={16} className="shrink-0 text-mut" />
                    <span className={`min-w-0 flex-1 text-[14px] ${isOpen ? 'font-semibold' : ''}`}>{x.title}</span>
                    <span className="flex shrink-0 items-center gap-1.5 text-[12px] text-mut"><span className="size-2 rounded-full" style={{ background: color(x.thread) }} />{x.thread} · {dayFmt(x.created)}</span>
                  </button>
                  {isOpen && (
                    <div className="px-5 pb-4 pl-12">
                      {x.body && <p className="max-w-[70ch] text-[13.5px] text-soft">{x.body}</p>}
                      <form onSubmit={(e) => submit(e, x.id)} className="mt-3 flex max-w-[620px] gap-2.5">
                        <input autoFocus className="nx-input" placeholder="Type your answer" value={ans} maxLength={320} onChange={(e) => setAns(e.target.value)} aria-label="Answer" />
                        <button className="btn primary !h-[42px]" type="submit">Answer</button>
                      </form>
                    </div>
                  )}
                </div>
              )
            })}
          </section>
        )}
      </div>
    </div>
  )
}
