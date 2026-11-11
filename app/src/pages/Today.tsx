import { useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { CheckCheck, Layers, MessageCircleQuestion, ScrollText, Sparkles } from 'lucide-react'
import { dayFmt, post } from '@/lib/api'
import { useShell } from '@/components/Shell'
import { STATE_HEX, STATE_NAME, dayCounts, nextStep, useDetails } from '@/lib/next'
import { Ring, SegRing, Spark } from '@/components/Viz'
import ItemActions, { Who } from '@/components/ItemActions'
import { whoActor } from '@/lib/api'
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

  const q = waiting[0]
  const qThread = threads.find((t) => t.name === q?.thread)
  const glow = STATE_HEX[qThread?.state ?? 'moving']

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!q) return
    const r = await post('/api/answer', { id: q.id, text: ans })
    if (r.ok) { setAns(''); reload() }
  }

  const when = new Date().toLocaleDateString('en-GB', { weekday: 'long', day: 'numeric', month: 'long' })

  return (
    <div className="nx-page">
      <div className="nx-in mb-5 flex items-end justify-between">
        <div><div className="text-[13px] text-mut">{when}</div><h2 className="text-[30px] font-semibold leading-tight tracking-tight">Today</h2></div>
      </div>

      <div className="nx-in mb-4 grid grid-cols-2 gap-3 text-[12.5px] text-soft max-[900px]:grid-cols-1">
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

      {q && (
        <section className="nx-now nx-in mt-5" style={{ ['--g' as any]: glow + '38', ['--d' as any]: '240ms' }}>
          <div className="relative flex items-start gap-4">
            <span className="nx-glyph size-12" style={{ background: glow + '22', color: glow }}><MessageCircleQuestion size={22} /></span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 text-[13px] text-mut"><span className="nx-chip !h-[18px] !px-2 !text-[11px]" style={{ background: '#60a5fa1f', color: '#60a5fa' }}>Claude asks you</span><span className="size-2 rounded-full" style={{ background: glow }} />{q.thread}<span>·</span>{dayFmt(q.created)}</div>
              <h3 className="mt-1 max-w-[30ch] text-[27px] font-semibold leading-[1.15] tracking-tight">{q.title}</h3>
              {q.body && <p className="mt-2 max-w-[60ch] text-[14px] text-soft">{q.body}</p>}
              <form onSubmit={submit} className="mt-5 flex max-w-[560px] gap-2.5">
                <input className="nx-input" placeholder="Type your answer" value={ans} maxLength={320} onChange={(e) => setAns(e.target.value)} />
                <button className="nx-btn" type="submit">Answer</button>
              </form>
            </div>
            <div className="hidden shrink-0 text-right text-[12.5px] text-mut lg:block">{waiting.length - 1} more waiting</div>
          </div>
        </section>
      )}

      {(today?.asks ?? []).length > 0 && (
        <section className="nx-panel nx-in mt-5">
          <div className="flex items-center justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold">Your questions for Claude</h3><span className="text-[12.5px] text-mut">{today.asks.length}</span></div>
          <p className="px-5 pb-2 text-[12.5px] text-mut">You asked these. Nothing to type here: Claude answers in its next session, and the answer appears on the project. Filed under the wrong project? Move it.</p>
          {today.asks.map((a: any) => (
            <div key={a.id} className="nx-row" style={{ gridTemplateColumns: 'minmax(0,1fr) auto', cursor: 'default' }}>
              <div className="min-w-0"><Link to={`/thread/${encodeURIComponent(a.thread)}?item=${a.id}`} className="block truncate text-[14px] font-medium hover:underline">{a.title}</Link><div className="truncate text-[12px] text-mut">{a.thread}{a.body ? ` · ${a.body}` : ''}</div></div>
              <ItemActions i={{ ...a, state: 'open', type: 'question' }} done={reload} />
            </div>
          ))}
        </section>
      )}

      <div className="mt-5 grid grid-cols-[minmax(0,1.55fr)_minmax(0,1fr)] gap-4 max-[1100px]:grid-cols-1">
        <section className="nx-panel nx-in" style={{ ['--d' as any]: '300ms' }}>
          <div className="flex items-center justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold">Moving</h3><span className="text-[12.5px] text-mut">{moving.length} threads</span></div>
          {moving.map((t) => {
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
          })}
        </section>

        <section className="nx-panel nx-in" style={{ ['--d' as any]: '360ms' }}>
          <div className="flex items-center justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold">Looks done</h3><span className="text-[12.5px] text-mut">{(today?.looks ?? []).length}</span></div>
          <p className="px-5 pb-2 text-[12.5px] text-mut">Steps that look built. Open a title to see it in its project; Confirm closes it, Not yet sends it back to to-do.</p>
          {(today?.looks ?? []).length === 0 && <div className="px-5 pb-4 text-[13px] text-mut">Nothing to confirm.</div>}
          {(today?.looks ?? []).map((l: any) => (
            <div key={l.id} className="nx-row" style={{ gridTemplateColumns: '32px minmax(0,1fr) auto', cursor: 'default' }}>
              <span className="nx-glyph size-8" style={{ background: '#34d3991f', color: '#34d399' }}><CheckCheck size={16} /></span>
              <div className="min-w-0">
                <Link to={`/thread/${encodeURIComponent(l.thread)}?item=${l.id}`} className="block truncate text-[13.5px] font-medium hover:underline">{l.title}</Link>
                <div className="truncate text-[12px] text-mut">{l.thread} · marked built by {whoActor(l)}</div>{l.answer && <div className="line-clamp-2 text-[12px]" style={{ color: '#34d399' }}>Evidence: {l.answer}</div>}
              </div>
              <div className="flex gap-2">
                <button className="btn sm primary" type="button" onClick={async () => { const r = await post('/api/tick', { id: l.id }); if (r.ok) reload() }}>Confirm</button>
                <button className="btn sm" type="button" onClick={async () => { const r = await post('/api/notyet', { id: l.id }); if (r.ok) reload() }}>Not yet</button>
              </div>
            </div>
          ))}
          <div className="flex items-center justify-between px-5 pb-1 pt-4"><h3 className="text-[15px] font-semibold">Suggested</h3><Link to="/items?state=proposed" className="text-[12.5px] text-mut hover:text-ink">See all {today?.suggested?.length ?? 0}</Link></div>
          <p className="px-5 pb-2 text-[12.5px] text-mut">Filed by Claude from sessions. Open a title to see it in its project. Accept keeps it, Not valid now parks it, Reject drops it.</p>
          {(today?.suggested ?? []).slice(0, 4).map((s: any) => (
            <div key={s.id} className="nx-row" style={{ gridTemplateColumns: '32px minmax(0,1fr) auto', cursor: 'default' }}>
              <span className="nx-glyph size-8" style={{ background: '#60a5fa1f', color: '#60a5fa' }}><Sparkles size={16} /></span>
              <div className="min-w-0">
                <Link to={`/thread/${encodeURIComponent(s.thread)}?item=${s.id}`} className="block truncate text-[13.5px] font-medium hover:underline">{s.title}</Link>
                <div className="flex items-center gap-2 truncate text-[12px] text-mut"><Who i={s} />{s.type} · {s.thread}</div>{s.body && <div className="line-clamp-2 text-[12px] text-soft">{s.body}</div>}
              </div>
              <ItemActions i={s} done={reload} />
            </div>
          ))}
        </section>
      </div>
    </div>
  )
}
