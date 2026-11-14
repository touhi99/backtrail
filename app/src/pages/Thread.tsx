import { useEffect, useMemo, useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import { Bug, Check, CheckCheck, CircleHelp, Copy, FileText, Lightbulb, MessageCircleQuestion, MessagesSquare, Network, Pin, Plus, Sparkles } from 'lucide-react'
import { dayFmt, post, STALE_LABEL, staleOf, useApi } from '@/lib/api'
import { useShell } from '@/components/Shell'
import { buildStory, JourneyRail, Made, Milestones, Settled } from '@/components/Story'
import ItemActions, { Who } from '@/components/ItemActions'
import More from '@/components/More'
import Paged from '@/components/Paged'
import Fold from '@/components/Fold'
import { SegRing } from '@/components/Viz'
import { FocusDeck, useLater, ts, WEEK } from '@/components/Focus'
import { STATE_HEX, STEP_STATUS } from '@/lib/next'
import '@/next.css'

const SRC_COLOR: Record<string, string> = { session: '#60a5fa', note: '#8b8b95', artifact: '#c084fc', commit: '#34d399', owner: '#f4f4f5' }
const DAY = 864e5

function Panel({ title, count, children, className = '', id }: { title: string; count?: number | string; children: React.ReactNode; className?: string; id?: string }) {
  return (
    <section id={id} className={`nx-panel nx-in overflow-hidden ${className}`}>
      <div className="flex items-baseline justify-between px-5 pb-2 pt-4"><h3 className="text-[15px] font-semibold">{title}</h3>{count != null && <span className="text-[12.5px] text-mut tabular-nums">{count}</span>}</div>
      {children}
    </section>
  )
}
const Empty = ({ children }: { children: React.ReactNode }) => <div className="px-5 pb-5 text-[13px] text-mut">{children}</div>
const Src = ({ i }: { i: any }) => i.source_id ? <Link to={`/source/${i.source_id}${i.chunk_id ? `?c=${i.chunk_id}` : ''}`} className="mt-1 inline-flex items-center gap-1.5 text-[12px] text-mut hover:text-ink"><FileText size={12} />{i.source_title}</Link> : null

type View = 'steps' | 'journey' | 'decisions' | 'made' | 'sources' | 'archive' | 'claude'
const VIEWS: [View, string][] = [['steps', 'All steps'], ['journey', 'Journey'], ['decisions', 'Decisions'], ['made', 'Made'], ['sources', 'Sources'], ['archive', 'Archive'], ['claude', 'Claude Code']]

export default function Thread() {
  const { name = '' } = useParams()
  const { data: d, reload } = useApi(`/api/thread?name=${encodeURIComponent(name)}`)
  const { capture, reload: reloadShell } = useShell()
  const [sp] = useSearchParams()
  const focus = sp.get('item')
  const [view, setView] = useState<View | null>(null)
  const { later } = useLater()
  useEffect(() => { setView(null) }, [name])
  useEffect(() => { const h = () => reload(); window.addEventListener('tm:changed', h); return () => window.removeEventListener('tm:changed', h) }, [reload])
  const story = useMemo(() => (d?.thread ? buildStory(d) : null), [d])

  const items: any[] = d?.items ?? []
  const now = Date.now()
  const queue = useMemo(() => {
    const checks = items.filter((i) => i.state === 'looks_done').sort((a, b) => (b.updated ?? '').localeCompare(a.updated ?? ''))
    const asks = items.filter((i) => i.type === 'question' && i.state === 'open' && i.origin !== 'owner')
    const rank: Record<string, number> = { step: 0, question: 1, decision: 2 }
    const sugg = items.filter((i) => i.state === 'proposed' && !staleOf(i) && now - ts(i) < WEEK).sort((a, b) => (rank[a.type] ?? 3) - (rank[b.type] ?? 3) || ts(b) - ts(a))
    return [...checks, ...asks, ...sugg].filter((i) => !later.includes(i.id))
  }, [items, later])

  useEffect(() => {
    if (!focus || !d) return
    const it = items.find((i) => i.id === focus)
    if (!it) return
    if (queue.some((q) => q.id === focus)) return
    setView(it.type === 'step' && it.state !== 'proposed' ? 'steps' : it.state === 'proposed' || it.state === 'rejected' || it.state === 'parked' ? 'archive' : it.type === 'decision' ? 'decisions' : 'steps')
    setTimeout(() => document.getElementById(`item-${focus}`)?.scrollIntoView({ block: 'center', behavior: 'smooth' }), 200)
  }, [focus, d])

  if (!d?.thread || !story) return <div className="p-8 text-mut">Loading</div>
  const th = d.thread
  const c = STATE_HEX[th.state] ?? '#8b8b95'
  const refresh = () => { reload(); reloadShell() }
  const steps = items.filter((i) => i.type === 'step' && !['proposed', 'rejected', 'parked'].includes(i.state))
  const doneN = steps.filter((s) => s.state === 'done').length
  const checkN = steps.filter((s) => s.state === 'looks_done').length
  const todo = steps.filter((s) => s.state === 'open' || s.state === 'doing').sort((a, b) => Number(b.state === 'doing') - Number(a.state === 'doing'))
  const recent = (t?: string) => !!t && now - Date.parse(t) < DAY
  const since = [
    { n: steps.filter((s) => s.state === 'done' && recent(s.updated)).length, one: 'step finished', many: 'steps finished', icon: Check, col: '#34d399' },
    { n: d.sources.filter((s: any) => s.kind === 'session' && recent(s.last_ts)).length, one: 'session', many: 'sessions', icon: MessagesSquare, col: '#c084fc' },
    { n: items.filter((i) => i.type === 'decision' && i.state !== 'rejected' && recent(i.source_ts)).length, one: 'decision noted', many: 'decisions noted', icon: Lightbulb, col: '#60a5fa' },
  ].filter((x) => x.n > 0)

  const prop = items.filter((i) => i.state === 'proposed')
  const archived = prop.filter((i) => staleOf(i) || now - ts(i) >= WEEK || later.includes(i.id)).sort((a, b) => ts(b) - ts(a))
  const rej = items.filter((i) => i.state === 'rejected' || i.state === 'parked')
  const asks = items.filter((i) => i.type === 'question' && i.state === 'open' && i.origin === 'owner')
  const copy = (t: string) => navigator.clipboard?.writeText(t)
  const hl = (id: string) => (id === focus ? { boxShadow: 'inset 0 0 0 1.5px #34d399', background: 'rgba(52,211,153,.07)' } : {})
  const tick = async (url: string, body: any) => { await post(url, body); refresh() }

  const stepRow = (s: any) => (
    <div key={s.id} id={`item-${s.id}`} className="nx-row" style={{ gridTemplateColumns: '24px minmax(0,1fr) auto', alignItems: 'start', cursor: 'default', ...hl(s.id) }}>
      <button type="button" aria-label="Done" onClick={() => tick('/api/tick', { id: s.id })} className="mt-0.5 grid size-5 place-items-center rounded-full border transition active:scale-90" style={{ borderColor: s.state === 'done' ? c : '#3a3a42', background: s.state === 'done' ? c : 'transparent' }}>
        {s.state === 'done' && <CheckCheck size={12} color="#04130c" strokeWidth={3} />}
      </button>
      <div className="min-w-0">
        <div className={`text-[14px] ${s.state === 'done' ? 'text-mut line-through' : 'font-medium'}`}>{s.tag === 'bug' && <Bug size={13} className="mr-1.5 inline text-dropped" />}{s.title}{STEP_STATUS[s.state] && <span title={STEP_STATUS[s.state].hint} className="nx-chip ml-2 align-middle" style={{ background: STEP_STATUS[s.state].color + '22', color: STEP_STATUS[s.state].color }}>{STEP_STATUS[s.state].label}</span>}</div>
        {(s.body || (s.state === 'done' && s.answer)) && <More label="Detail" className="mr-3 text-[12.5px] text-mut">{s.body}{s.state === 'done' && s.answer ? <span style={{ color: c }}> {s.answer}</span> : null}</More>}
        <Src i={s} />
        {s.origin === 'owner' && s.state !== 'done' && <div className="mt-1.5"><ItemActions i={s} done={refresh} /></div>}
      </div>
      {(s.state === 'open' || s.state === 'doing') && <button className={`btn sm ${s.state === 'doing' ? '' : 'ghost'}`} onClick={() => tick('/api/doing', { id: s.id })}>{s.state === 'doing' ? 'Back to to-do' : 'Mark in progress'}</button>}
    </div>
  )
  const propRow = (i: any) => {
    const st = staleOf(i)
    return (
      <div key={i.id} id={`item-${i.id}`} className="nx-row" style={{ gridTemplateColumns: '32px minmax(0,1fr) auto', cursor: 'default', ...hl(i.id) }}>
        <span className="nx-glyph size-8" style={{ background: '#60a5fa1f', color: '#60a5fa' }}><Sparkles size={15} /></span>
        <div className="min-w-0"><div className="text-[14px] font-medium">{i.title}</div>
          <div className="mt-0.5 text-[12px] text-mut">{dayFmt(i.source_ts ?? i.created)}{st && <> · <span className="text-open" title={st.reason}>{STALE_LABEL[st.kind]}</span></>}</div>{i.body && <More label="Detail" className="text-[12.5px] text-mut">{i.body}</More>}</div>
        <ItemActions i={i} done={refresh} />
      </div>
    )
  }

  return (
    <div className="nx-page">
      <div className="pointer-events-none absolute inset-x-0 top-0 h-[420px]" style={{ background: 'radial-gradient(900px 420px at 30% -80px, rgba(52,211,153,.09), transparent 70%)' }} />
      <div className="relative mx-auto flex max-w-[1080px] flex-col gap-7">
        <header className="nx-in flex flex-wrap items-center gap-3.5">
          <span className="nx-glyph size-11 text-[18px] font-semibold" style={{ background: c + '1f', color: c, boxShadow: `0 0 30px ${c}33` }}>{th.name[0]}</span>
          <div className="min-w-0 flex-1">
            <h2 className="text-[26px] font-semibold leading-tight tracking-tight">{th.name}</h2>
            {th.summary && <p className="truncate text-[13.5px] text-mut">{th.summary}</p>}
          </div>
          <Link className="btn ghost" to={`/map?thread=${encodeURIComponent(th.name)}`}><Network size={14} />Map</Link>
          <button className="btn" onClick={() => capture(th.name)}><Plus size={14} />Add</button>
        </header>

        <div className="flex flex-wrap items-stretch gap-5">
          <FocusDeck queue={queue} refresh={refresh} resetKey={name} focus={focus} />

          <aside className="flex flex-col gap-3" style={{ flex: '1 1 260px' }}>
            <div className="nx-panel nx-in flex items-center gap-4 !rounded-2xl p-[18px]">
              <div className="relative grid place-items-center" title={`${doneN} confirmed, ${checkN} built and waiting for your OK, ${steps.length - doneN - checkN} open`}>
                <SegRing size={84} stroke={9} segs={[{ v: doneN, c: '#34d399' }, { v: checkN, c: '#fbbf24' }, { v: Math.max(steps.length - doneN - checkN, 0), c: 'transparent' }]} />
                <span className="absolute text-[17px] font-semibold">{steps.length ? Math.round(((doneN + checkN) / steps.length) * 100) : 0}%</span>
              </div>
              <div><div className="text-[26px] font-semibold tracking-tight tabular-nums">{doneN + checkN}<span className="text-[18px] text-mut">/{steps.length}</span></div><div className="text-[13px] text-mut">steps built</div></div>
            </div>
            <div className="px-1 pt-1.5 text-[12.5px] text-mut">Since yesterday</div>
            {since.length === 0 && <div className="nx-panel !rounded-2xl px-4 py-3.5 text-[14px] text-soft">Quiet</div>}
            {since.map((x) => (
              <div key={x.one} className="nx-panel nx-in flex items-center gap-3 !rounded-2xl px-4 py-3">
                <span className="grid size-[34px] place-items-center rounded-[10px]" style={{ background: x.col + '24', color: x.col }}><x.icon size={17} strokeWidth={2.2} /></span>
                <div className="text-[15px] font-semibold">{x.n} {x.n === 1 ? x.one : x.many}</div>
              </div>
            ))}
          </aside>
        </div>

        <section className="nx-in">
          <div className="mb-3 flex items-baseline justify-between"><h3 className="text-[17px] font-semibold">Up next</h3><button className="text-[13px] text-mut hover:text-ink" onClick={() => setView('steps')}>All steps</button></div>
          {todo.length === 0 ? <div className="text-[13.5px] text-mut">No open steps.</div> : (
            <div className="grid grid-cols-3 gap-3 max-[760px]:grid-cols-1">
              {todo.slice(0, 3).map((s, n) => (
                <button key={s.id} onClick={() => { setView('steps'); setTimeout(() => document.getElementById(`item-${s.id}`)?.scrollIntoView({ block: 'center', behavior: 'smooth' }), 120) }}
                  className="nx-panel flex min-h-[108px] flex-col gap-2.5 !rounded-2xl px-4 py-3.5 text-left transition hover:-translate-y-px hover:border-[#34343c]" style={n === 0 ? { borderColor: '#2b3a33' } : {}}>
                  <span className="size-2 rounded-full" style={n === 0 ? { background: '#34d399', boxShadow: '0 0 10px #34d399' } : { background: '#3f3f46' }} />
                  <span className="line-clamp-3 text-[15px] font-semibold leading-snug">{s.title}</span>
                </button>
              ))}
            </div>
          )}
        </section>

        <nav className="nx-in flex flex-wrap gap-2">
          {VIEWS.map(([v, l]) => <button key={v} className={`btn ${view === v ? '' : 'ghost'} !border-[#26262c]`} aria-pressed={view === v} onClick={() => setView(view === v ? null : v)}>{l}</button>)}
        </nav>

        {view === 'steps' && (
          <div className="flex flex-col gap-4">
            {asks.map((i) => (
              <section key={i.id} id={`item-${i.id}`} className="nx-panel nx-in px-5 py-4" style={hl(i.id)}>
                <div className="flex items-start gap-3"><CircleHelp size={18} className="mt-0.5 text-soft" />
                  <div className="min-w-0 flex-1"><div className="flex items-center gap-2 text-[12px] text-mut"><Who i={i} />your question · {dayFmt(i.created)}</div><h3 className="mt-1 text-[16px] font-semibold">{i.title}</h3>{i.body && <p className="mt-1 text-[13px] text-soft">{i.body}</p>}</div></div>
              </section>
            ))}
            <Panel title="Steps" count={`${doneN} of ${steps.length} done`}>
              {steps.length === 0 && <Empty>No steps.</Empty>}
              {focus ? steps.filter((s) => s.state !== 'done').map(stepRow) : <Paged resetKey={name} items={steps.filter((s) => s.state !== 'done')} size={10}>{(rows: any[]) => rows.map(stepRow)}</Paged>}
              <Fold key={focus ?? ''} label="Done" count={doneN} defaultOpen={!!focus && steps.some((s) => s.id === focus && s.state === 'done')}>{focus ? steps.filter((s) => s.state === 'done').map(stepRow) : <Paged resetKey={name} items={steps.filter((s) => s.state === 'done')} size={10}>{(rows: any[]) => rows.map(stepRow)}</Paged>}</Fold>
            </Panel>
          </div>
        )}
        {view === 'journey' && <div className="flex flex-col gap-4"><JourneyRail s={story} color={c} /><Milestones s={story} color={c} /></div>}
        {view === 'decisions' && <Settled s={story} done={refresh} />}
        {view === 'made' && <Made s={story} />}
        {view === 'sources' && (
          <Panel title="Sources" count={d.sources.length}>
            <Paged resetKey={name} items={d.sources} size={12}>{(rows: any[]) => rows.map((s: any) => <Link key={s.id} to={`/source/${s.id}`} className="nx-row" style={{ gridTemplateColumns: '10px minmax(0,1fr) auto', padding: '10px 18px' }}><span className="size-2 rounded-full" style={{ background: SRC_COLOR[s.kind] }} /><span className="truncate text-[13.5px]">{s.title}</span><span className="text-[12px] text-mut">{dayFmt(s.last_ts)}</span></Link>)}</Paged>
          </Panel>
        )}
        {view === 'archive' && (
          <div className="flex flex-col gap-4">
            <Panel title="Older suggestions" count={archived.length}>
              {archived.length === 0 && <Empty>Nothing here.</Empty>}
              {archived.map(propRow)}
            </Panel>
            {rej.length > 0 && <section className="nx-panel nx-in overflow-hidden"><Fold label="Rejected or not valid now" count={rej.length}>{rej.map((i) => <div key={i.id} id={`item-${i.id}`} className="nx-row" style={{ gridTemplateColumns: 'minmax(0,1fr)', cursor: 'default', ...hl(i.id) }}><div><div className="text-[14px] text-mut line-through">{i.title}</div><More label={i.state === 'parked' ? 'Why not valid' : 'Why rejected'} className="text-[12.5px] text-mut">{i.answer || 'No reason given.'}</More></div></div>)}</Fold></section>}
          </div>
        )}
        {view === 'claude' && (
          <Panel title="Work on it in Claude Code">
            <div className="flex flex-wrap items-center gap-2 px-5 pb-4">
              <code className="min-w-0 flex-1 truncate rounded-xl border border-line bg-[#0c0c0e] px-3.5 py-2.5 text-[12px] text-soft">claude "$(backtrail brief --thread '{th.name}')"</code>
              <button className="btn" onClick={() => copy(`claude "$(backtrail brief --thread '${th.name}')"`)}><Copy size={13} />Copy</button>
              {['Review', 'Extend', 'Explore'].map((v) => <button key={v} className="btn" onClick={() => copy(`${v} the Backtrail project "${th.name}". Use backtrail brief, search and show; file proposed items with a source and a verbatim quote (skill/SKILL.md).`)}>{v}</button>)}
            </div>
          </Panel>
        )}
      </div>
    </div>
  )
}
