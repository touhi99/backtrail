import { useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { CheckCheck, Layers, MessageCircleQuestion, ScrollText, Sparkles } from 'lucide-react'
import { dayFmt, post } from '@/lib/api'
import { useShell } from '@/components/Shell'
import { STATE_HEX, STATE_NAME, dayCounts, nextStep, useDetails } from '@/lib/next'
import { Ring, SegRing, Spark } from '@/components/Viz'
import ItemActions, { Who } from '@/components/ItemActions'
import Paged from '@/components/Paged'
import { whoActor } from '@/lib/api'
import { TrailCard } from '@/pages/Trail'
import '@/next.css'

const PALETTE = ['#60a5fa', '#34d399', '#fbbf24', '#c084fc', '#fb7185', '#2dd4bf', '#8b8b95']

function Tile({ label, icon: Icon, d, children }: { label: string; icon: any; d: number; children: React.ReactNode }) {
  return <div className="nx-tile nx-in" style={{ ['--d' as any]: `${d}ms` }}><div className="nx-label"><Icon size={15} />{label}</div>{children}</div>
}

export default function Today() {
  const { today, reload } = useShell()
  const [ans, setAns] = useState('')
  const threads: any[] = today?.threads ?? []
  const moving = threads.filter((t) => t.state === 'moving')
  const waiting: any[] = today?.waiting ?? []
  const details = useDetails(threads.map((t) => t.name))

  const byProject = useMemo(() => {
    const m: Record<string, number> = {}
    waiting.forEach((q) => { m[q.thread ?? 'Other'] = (m[q.thread ?? 'Other'] ?? 0) + 1 })
    return Object.entries(m).sort((a, b) => b[1] - a[1])
  }, [waiting])
  const states = useMemo(() => {
    const m: Record<string, number> = {}
    threads.forEach((t) => { m[t.state] = (m[t.state] ?? 0) + 1 })
    return ['moving', 'built', 'parked', 'dropped', 'open'].filter((s) => m[s]).map((s) => ({ s, v: m[s] }))
  }, [threads])
  const sessionDays = useMemo(() => {
    const seen = new Set<number>(), out = Array(30).fill(0)
    Object.values(details).forEach((d: any) => (d?.sources ?? []).forEach((s: any) => { if (s.kind === 'session' && !seen.has(s.id)) seen.add(s.id) }))
    const all: any[] = []
    Object.values(details).forEach((d: any) => (d?.sources ?? []).forEach((s: any) => all.push(s)))
    const uniq = Array.from(new Map(all.filter((s) => s.kind === 'session').map((s) => [s.id, s])).values())
    const c = dayCounts({ sources: uniq }, 30)
    c.forEach((v, i) => (out[i] = v))
    return out as number[]
  }, [details])
  const done = threads.reduce((a, t) => a + (t.done_s ?? 0), 0), open = threads.reduce((a, t) => a + (t.open_s ?? 0), 0)

  const [later, setLater] = useState<number[]>(() => { try { return JSON.parse(localStorage.getItem('bt-later') ?? '[]') } catch { return [] } })
  const [openQ, setOpenQ] = useState<number | null>(null)
  const [allQ, setAllQ] = useState(false)
  const saveLater = (v: number[]) => { setLater(v); try { localStorage.setItem('bt-later', JSON.stringify(v)) } catch {} }
  const ordered = [...waiting.filter((x) => !later.includes(x.id)), ...later.map((id) => waiting.find((x) => x.id === id)).filter(Boolean)]
  const skip = (id: number) => { setOpenQ(null); setAns(''); saveLater([...later.filter((x) => x !== id), id]) }

  const submit = async (e: React.FormEvent, id: number) => {
    e.preventDefault()
    const r = await post('/api/answer', { id, text: ans })
    if (r.ok) { setAns(''); setOpenQ(null); reload() }
  }

  const when = new Date().toLocaleDateString('en-GB', { weekday: 'long', day: 'numeric', month: 'long' })


  const nLooks = (today?.looks ?? []).length, nSug = (today?.suggested ?? []).length
  const nothing = waiting.length + nLooks + nSug === 0
  const [shown, setShown] = useState<number[]>([])
  const flip = (id: number) => setShown((v) => (v.includes(id) ? v.filter((x) => x !== id) : [...v, id]))
  const parts = [waiting.length && `${waiting.length} ${waiting.length === 1 ? 'question' : 'questions'} to answer`, nLooks && `${nLooks} marked done by Claude`, nSug && `${nSug} suggestions`].filter(Boolean)

  return (
    <div className="nx-page">
      <div className="nx-in mb-5">
        <div className="text-[13px] text-mut">{when}</div>
        <h2 className="text-[30px] font-semibold leading-tight tracking-tight">Today</h2>
        <p className="mt-1 text-[14px] text-soft">{nothing ? 'Nothing is waiting on you.' : `Waiting on you: ${parts.join(' · ')}.`}</p>
      </div>

      {nothing && (
        <section className="nx-panel nx-in grid place-items-center px-5 py-10 text-center">
          <CheckCheck size={26} className="text-accent" />
          <div className="mt-3 text-[16px] font-semibold">All clear</div>
          <div className="mt-1 text-[13px] text-mut">Claude files new questions and suggestions after each session.</div>
        </section>
      )}
      <TrailCard />
      {ordered.length > 0 && (
        <section className="nx-panel nx-in mt-5" style={{ ['--d' as any]: '300ms' }}>
          <div className="flex items-baseline justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold">Questions for you</h3><span className="text-[12.5px] text-mut">{ordered.length}</span></div>
          {(allQ ? ordered : ordered.slice(0, 4)).map((x) => {
            const c = STATE_HEX[threads.find((t) => t.name === x.thread)?.state ?? 'moving'], isOpen = openQ === x.id
            return (
              <div key={x.id} className="border-t border-line first-of-type:border-t-0">
                <button type="button" onClick={() => { setOpenQ(isOpen ? null : x.id); setAns('') }} className="row-hover flex w-full items-center gap-3 px-5 py-3 text-left">
                  <MessageCircleQuestion size={16} className="shrink-0 text-mut" />
                  <span className={`min-w-0 flex-1 text-[14px] ${isOpen ? 'font-semibold' : ''}`}>{x.title}</span>
                  <span className="flex shrink-0 items-center gap-1.5 text-[12px] text-mut"><span className="size-2 rounded-full" style={{ background: c }} />{x.thread}{later.includes(x.id) ? ', later' : ''}</span>
                </button>
                {isOpen && (
                  <div className="px-5 pb-4 pl-12">
                    {x.body && <p className="max-w-[70ch] text-[13.5px] text-soft">{x.body}</p>}
                    <div className="mt-1 text-[12px] text-mut">Asked {dayFmt(x.created)}</div>
                    <form onSubmit={(e) => submit(e, x.id)} className="mt-3 flex max-w-[620px] gap-2.5">
                      <input autoFocus className="nx-input" placeholder="Type your answer" value={ans} maxLength={320} onChange={(e) => setAns(e.target.value)} />
                      <button className="nx-btn" type="submit">Answer</button>
                      <button className="btn" type="button" onClick={() => skip(x.id)} title="Moves this question to the end of the list">Not now</button>
                    </form>
                  </div>
                )}
              </div>
            )
          })}
          {ordered.length > 4 && <button type="button" onClick={() => setAllQ(!allQ)} className="w-full border-t border-line px-5 py-2.5 text-left text-[12.5px] font-medium text-accent hover:underline">{allQ ? 'Show fewer' : `Show all ${ordered.length}`}</button>}
        </section>
      )}
        <div className="mt-5 grid grid-cols-2 items-start gap-4 max-[1100px]:grid-cols-1">
        <section className="nx-panel nx-in" style={{ ['--d' as any]: '300ms' }}>
          <div className="flex items-center justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold" title="Steps that Claude found built. Confirm closes a step; Not yet sends it back to to-do.">Claude says it's done</h3><span className="text-[12.5px] text-mut">{(today?.looks ?? []).length}</span></div>
          <Paged items={today?.looks ?? []} size={5}>{(pageRows: any[]) => pageRows.map((l: any) => (
            <div key={l.id} className="nx-row" style={{ gridTemplateColumns: '32px minmax(0,1fr) auto', cursor: 'default' }}>
              <span className="nx-glyph size-8" style={{ background: '#34d3991f', color: '#34d399' }}><CheckCheck size={16} /></span>
              <div className="min-w-0">
                <Link to={`/thread/${encodeURIComponent(l.thread)}?item=${l.id}`} className="block truncate text-[13.5px] font-medium hover:underline">{l.title}</Link>
                <div className="truncate text-[12px] text-mut">{l.thread}</div>{l.answer && (shown.includes(l.id) ? <div className="text-[12px]" style={{ color: '#34d399' }}>{l.answer}</div> : <button type="button" className="text-[12px] text-mut hover:text-ink" onClick={() => flip(l.id)}>Show evidence</button>)}
              </div>
              <div className="flex gap-2">
                <button className="btn sm primary" type="button" onClick={async () => { const r = await post('/api/tick', { id: l.id }); if (r.ok) reload() }}>Confirm</button>
                <button className="btn sm" type="button" onClick={async () => { const r = await post('/api/notyet', { id: l.id }); if (r.ok) reload() }}>Not yet</button>
              </div>
            </div>
          ))}</Paged>
        </section>
        <section className="nx-panel nx-in" style={{ ['--d' as any]: '340ms' }}>
          <div className="flex items-center justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold" title="Decisions, steps and questions Claude found in sessions. Accept keeps one, Not valid now parks it, Reject drops it.">Claude suggests</h3><Link to="/items?state=proposed" className="text-[12.5px] text-mut hover:text-ink">See all {today?.suggested?.length ?? 0}</Link></div>
          {(today?.suggested ?? []).slice(0, 5).map((s: any) => (
            <div key={s.id} className="nx-row" style={{ gridTemplateColumns: '32px minmax(0,1fr) auto', cursor: 'default' }}>
              <span className="nx-glyph size-8" style={{ background: '#60a5fa1f', color: '#60a5fa' }}><Sparkles size={16} /></span>
              <div className="min-w-0">
                <Link to={`/thread/${encodeURIComponent(s.thread)}?item=${s.id}`} className="block truncate text-[13.5px] font-medium hover:underline">{s.title}</Link>
                <div className="flex items-center gap-2 truncate text-[12px] text-mut"><Who i={s} />{s.type} · {s.thread}</div>{s.body && (shown.includes(s.id) ? <div className="text-[12px] text-soft">{s.body}</div> : <button type="button" className="text-[12px] text-mut hover:text-ink" onClick={() => flip(s.id)}>Show detail</button>)}
              </div>
              <ItemActions i={s} done={reload} />
            </div>
          ))}
        </section>
        </div>
      {(today?.asks ?? []).length > 0 && (
        <section className="nx-panel nx-in mt-5">
          <div className="flex items-center justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold" title="You asked these. Claude answers in its next session and the answer appears on the project.">Your questions for Claude</h3><span className="text-[12.5px] text-mut">{today.asks.length}</span></div>
          {today.asks.map((a: any) => (
            <div key={a.id} className="nx-row" style={{ gridTemplateColumns: 'minmax(0,1fr) auto', cursor: 'default' }}>
              <div className="min-w-0"><Link to={`/thread/${encodeURIComponent(a.thread)}?item=${a.id}`} className="block truncate text-[14px] font-medium hover:underline">{a.title}</Link><div className="truncate text-[12px] text-mut">{a.thread}{a.body ? ` · ${a.body}` : ''}</div></div>
              <ItemActions i={{ ...a, state: 'open', type: 'question' }} done={reload} />
            </div>
          ))}
        </section>
      )}
        <section className="nx-panel nx-in mt-5" style={{ ['--d' as any]: '300ms' }}>
          <div className="flex items-center justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold">In development</h3><span className="text-[12.5px] text-mut">{moving.length} threads</span></div>
          <div className="grid grid-cols-2 max-[1100px]:grid-cols-1">{moving.map((t) => {
            const dd = details[t.name], act = dayCounts(dd, 14), mx = Math.max(1, ...act), c = STATE_HEX[t.state]
            const tot = t.done_s + t.open_s, nx = nextStep(dd)
            return (
              <Link key={t.name} to={`/thread/${encodeURIComponent(t.name)}`} className="nx-row" style={{ gridTemplateColumns: '40px minmax(0,1fr) auto 46px' }}>
                <span className="nx-glyph size-10 text-[15px] font-semibold" style={{ background: c + '1f', color: c }}>{t.name[0]}</span>
                <div className="min-w-0">
                  <div className="flex items-center gap-2 text-[14.5px] font-medium">{t.name}{t.open_q > 0 && <span className="nx-chip" style={{ background: '#fbbf2420', color: '#fbbf24' }}>{t.open_q} waiting</span>}</div>
                  <div className="truncate text-[12.5px] text-mut">{nx ?? t.summary}</div>
                </div>
                <div className="flex items-end gap-[3px]" title="Sources in the last 14 days">
                  {act.map((v, i) => <span key={i} className="nx-pill" style={{ height: 6 + (v / mx) * 16, background: v ? c : '#26262b', opacity: v ? 0.45 + 0.55 * (v / mx) : 1 }} />)}
                </div>
                <Ring value={tot ? t.done_s / tot : 0} color={c} size={42} stroke={4.5}><span className="text-[10.5px] font-semibold tabular-nums text-soft">{t.done_s}/{tot}</span></Ring>
              </Link>
            )
          })}</div>
        </section>
      <details className="nx-in mt-6">
        <summary className="cursor-pointer text-[13px] text-mut hover:text-ink">Overview</summary>
      <div className="nx-in mb-4 mt-3 grid grid-cols-2 gap-3 text-[12.5px] text-soft max-[900px]:grid-cols-1">
        <div className="rounded-xl border border-line px-4 py-2.5"><span className="nx-chip mr-2" style={{ background: '#f4f4f51a', color: '#f4f4f5' }}>You</span>type todos, bugs and questions · answer Claude's questions · confirm built steps · accept or reject suggestions</div>
        <div className="rounded-xl border border-line px-4 py-2.5"><span className="nx-chip mr-2" style={{ background: '#60a5fa1f', color: '#60a5fa' }}>Claude</span>files suggestions from sessions · marks steps built with evidence · answers your questions · asks you when blocked</div>
      </div>
      <div className="grid grid-cols-4 gap-4 max-[1100px]:grid-cols-2">
        <Tile label="Waiting on you" icon={MessageCircleQuestion} d={0}>
          <div className="mt-2 flex items-center justify-between"><span className="nx-big">{waiting.length}</span>{byProject[0] && <span className="nx-delta">{byProject[0][0]} {byProject[0][1]}</span>}</div>
          <div className="mt-1 text-[12px] text-mut">Questions Claude asked you</div>
          <div className="mt-4 flex h-2 gap-[3px] overflow-hidden rounded-full">
            {byProject.map(([n, v], i) => <span key={n} title={`${n}: ${v}`} style={{ flex: v, background: PALETTE[i % PALETTE.length] }} className="rounded-full" />)}
          </div>
          <div className="mt-2.5 truncate text-[12px] text-mut">{byProject.length} projects</div>
        </Tile>

        <Tile label="Threads" icon={Layers} d={60}>
          <div className="mt-2 flex items-center gap-4">
            <div className="relative grid place-items-center"><SegRing segs={states.map((x) => ({ v: x.v, c: STATE_HEX[x.s] }))} /><span className="absolute text-[22px] font-semibold tracking-tight">{threads.length}</span></div>
            <div className="grid gap-1 text-[12px]">
              {states.map((x) => <div key={x.s} className="flex items-center gap-2 text-soft"><span className="size-2 rounded-full" style={{ background: STATE_HEX[x.s] }} />{STATE_NAME[x.s]}<span className="ml-auto pl-3 text-mut tabular-nums">{x.v}</span></div>)}
            </div>
          </div>
        </Tile>

        <Tile label="Sessions" icon={ScrollText} d={120}>
          <div className="mt-2 flex items-center justify-between"><span className="nx-big">{today?.totals?.sessions ?? ''}</span><span className="nx-delta">{sessionDays.slice(-7).reduce((a, b) => a + b, 0)} this week</span></div>
          <div className="mt-2.5"><Spark data={sessionDays} /></div>
          <div className="mt-1 flex justify-between text-[11.5px] text-mut"><span>30 days ago</span><span>today</span></div>
        </Tile>

        <Tile label="Steps done" icon={CheckCheck} d={180}>
          <div className="mt-2 flex items-center justify-between"><span className="nx-big">{done}<span className="text-[18px] font-medium text-mut"> / {done + open}</span></span></div>
          <div className="mt-5 h-2 overflow-hidden rounded-full bg-[#1d1d21]"><div className="h-full rounded-full" style={{ width: `${(done / Math.max(done + open, 1)) * 100}%`, background: 'linear-gradient(90deg,#34d399,#2dd4bf)' }} /></div>
          <div className="mt-2.5 text-[12px] text-mut">{open} still open</div>
        </Tile>
      </div>
      </details>

    </div>
  )
}
