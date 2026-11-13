import { useEffect, useMemo, useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import { Bug, CheckCheck, CircleHelp, Copy, FileText, Layers, MessageCircleQuestion, Network, Plus, Scale, ScrollText, Sparkles } from 'lucide-react'
import { agoFmt, dayFmt, post, STALE_LABEL, staleOf, useApi, whoActor } from '@/lib/api'
import { useShell } from '@/components/Shell'
import { Ring, Spark } from '@/components/Viz'
import ItemActions, { Who } from '@/components/ItemActions'
import More from '@/components/More'
import Paged from '@/components/Paged'
import Fold from '@/components/Fold'
import { dayCounts, STATE_HEX, STATE_NAME, STEP_STATUS } from '@/lib/next'
import '@/next.css'

const SRC_COLOR: Record<string, string> = { session: '#60a5fa', note: '#8b8b95', artifact: '#c084fc', commit: '#34d399', owner: '#f4f4f5' }
const SRC_NAME: Record<string, string> = { session: 'Sessions', note: 'Notes', artifact: 'Artifacts', commit: 'Commits', owner: 'Typed here' }

function Panel({ title, count, children, d = 0, className = '', id, hint }: { hint?: string; title: string; count?: number | string; children: React.ReactNode; d?: number; className?: string; id?: string }) {
  return (
    <section id={id} className={`nx-panel nx-in overflow-hidden ${className}`} style={{ ['--d' as any]: `${d}ms` }}>
      <div className="flex items-baseline justify-between px-5 pb-2 pt-4"><h3 className="text-[15px] font-semibold" title={hint}>{title}</h3>{count != null && <span className="text-[12.5px] text-mut tabular-nums">{count}</span>}</div>
      {children}
    </section>
  )
}
const Empty = ({ children }: { children: React.ReactNode }) => <div className="px-5 pb-5 text-[13px] text-mut">{children}</div>
const Src = ({ i }: { i: any }) => i.source_id ? <Link to={`/source/${i.source_id}${i.chunk_id ? `?c=${i.chunk_id}` : ''}`} className="mt-1 inline-flex items-center gap-1.5 text-[12px] text-mut hover:text-ink"><FileText size={12} />{i.source_title}</Link> : null

/** Sources per day over the thread's life, stacked by kind. */
function Journey({ d }: { d: any }) {
  const { days, max } = useMemo(() => {
    const m: Record<string, Record<string, number>> = {}
    d.sources.forEach((x: any) => { const k = (x.first_ts || x.last_ts || '').slice(0, 10); if (k > '2000') { m[k] ||= {}; m[k][x.kind] = (m[k][x.kind] ?? 0) + 1 } })
    const ks = Object.keys(m).sort()
    if (!ks.length) return { days: [], max: 1 }
    const out: { k: string; c: Record<string, number> }[] = []
    for (let t = new Date(ks[0] + 'T12:00:00'); t <= new Date(ks[ks.length - 1] + 'T12:00:00'); t.setDate(t.getDate() + 1)) { const k = t.toISOString().slice(0, 10); out.push({ k, c: m[k] ?? {} }) }
    return { days: out, max: Math.max(1, ...out.map((x) => Object.values(x.c).reduce((a, b) => a + b, 0))) }
  }, [d])
  if (!days.length) return <Empty>No sources yet.</Empty>
  const kinds = Object.keys(SRC_COLOR).filter((k) => d.sources.some((x: any) => x.kind === k))
  return (
    <div className="px-5 pb-4">
      <div className="flex h-[104px] items-end gap-[4px] pt-5">
        {days.map((x) => {
          const tot = Object.values(x.c).reduce((a, b) => a + b, 0)
          return (
            <div key={x.k} title={`${dayFmt(x.k)}: ${Object.entries(x.c).map(([k, v]) => `${v} ${SRC_NAME[k] ?? k}`).join(', ') || 'nothing'}`} className="relative flex min-w-[6px] max-w-[44px] flex-1 flex-col-reverse gap-px" style={{ height: `${Math.max(4, (tot / max) * 100)}%`, opacity: tot ? 1 : 0.35 }}>
              {tot > 0 && <span className="absolute -top-4 left-0 right-0 text-center text-[11px] tabular-nums text-soft">{tot}</span>}{tot === 0 ? <span className="h-full rounded-[3px] bg-[#26262b]" /> : Object.entries(x.c).map(([k, v]) => <span key={k} className="rounded-[3px]" style={{ flex: v, background: SRC_COLOR[k] ?? '#8b8b95' }} />)}
            </div>
          )
        })}
      </div>
      <div className="mt-2 flex items-center justify-between text-[12px] text-mut">
        <span>{dayFmt(days[0].k)}</span>
        <div className="flex gap-4">{kinds.map((k) => <span key={k} className="flex items-center gap-1.5"><span className="size-2 rounded-full" style={{ background: SRC_COLOR[k] }} />{SRC_NAME[k]}</span>)}</div>
        <span>{dayFmt(days[days.length - 1].k)}</span>
      </div>
    </div>
  )
}

export default function Thread() {
  const { name = '' } = useParams()
  const { data: d, reload } = useApi(`/api/thread?name=${encodeURIComponent(name)}`)
  const { capture, reload: reloadShell } = useShell()
  const [ans, setAns] = useState<Record<string, string>>({})
  const [sp] = useSearchParams()
  const focus = sp.get('item')
  useEffect(() => { if (focus && d?.items) setTimeout(() => document.getElementById(`item-${focus}`)?.scrollIntoView({ block: 'center', behavior: 'smooth' }), 150) }, [focus, d])
  const hl = (id: string) => (id === focus ? { boxShadow: 'inset 0 0 0 1.5px #34d399', background: 'rgba(52,211,153,.07)' } : {})
  useEffect(() => { const h = () => reload(); window.addEventListener('tm:changed', h); return () => window.removeEventListener('tm:changed', h) }, [reload])
  if (!d?.thread) return <div className="p-8 text-mut">Loading</div>
  const th = d.thread
  const c = STATE_HEX[th.state] ?? '#8b8b95'
  const live = d.items.filter((i: any) => !['proposed', 'rejected', 'parked'].includes(i.state))
  const by = (t: string) => live.filter((i: any) => i.type === t)
  const open = by('question').filter((i: any) => i.state === 'open' && i.origin !== 'owner')
  const asks = by('question').filter((i: any) => i.state === 'open' && i.origin === 'owner')
  const answered = by('question').filter((i: any) => i.state !== 'open')
  const steps = by('step')
  const stepsDone = steps.filter((s: any) => s.state === 'done').length
  const prop = d.items.filter((i: any) => i.state === 'proposed')
  const rej = d.items.filter((i: any) => i.state === 'rejected' || i.state === 'parked')
  const act = async (url: string, body: any) => { await post(url, body); reload(); reloadShell() }
  const copy = (t: string) => navigator.clipboard?.writeText(t)
  const spark = dayCounts(d, 30)
  const last = d.sources.map((s: any) => s.last_ts).filter(Boolean).sort().pop()
  const decisions = by('decision').filter((i: any) => !(i.stale ?? '').startsWith('superseded'))

  const openSteps = steps.filter((x: any) => x.state !== 'done'), doneSteps = steps.filter((x: any) => x.state === 'done')
  const stepRow = (s: any) => (
            <div key={s.id} id={`item-${s.id}`} className="nx-row" style={{ gridTemplateColumns: '24px minmax(0,1fr) auto', alignItems: 'start', cursor: 'default', ...hl(s.id) }}>
              <button type="button" aria-label="Done" onClick={() => act('/api/tick', { id: s.id })} className="mt-0.5 grid size-5 place-items-center rounded-full border transition active:scale-90" style={{ borderColor: s.state === 'done' ? c : '#3a3a42', background: s.state === 'done' ? c : 'transparent' }}>
                {s.state === 'done' && <CheckCheck size={12} color="#04130c" strokeWidth={3} />}
              </button>
              <div className="min-w-0">
                <div className={`text-[14px] ${s.state === 'done' ? 'text-mut line-through' : 'font-medium'}`}>{s.tag === 'bug' && <Bug size={13} className="mr-1.5 inline text-dropped" />}{s.title}{STEP_STATUS[s.state] && <span title={STEP_STATUS[s.state].hint} className="nx-chip ml-2 align-middle" style={{ background: STEP_STATUS[s.state].color + '22', color: STEP_STATUS[s.state].color }}>{STEP_STATUS[s.state].label}</span>}</div>
                <div className="mt-0.5 flex items-center gap-1.5 text-[11.5px] text-mut"><Who i={s} />{dayFmt(s.created)}</div>
                {(s.body || (s.state === 'done' && s.answer)) && <More label="Detail" className="mr-3 text-[12.5px] text-mut">{s.body}{s.state === 'done' && s.answer ? <span style={{ color: c }}> {s.answer}</span> : null}</More>}
                {s.state === 'looks_done' && <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[12px]" style={{ color: '#34d399' }}>Built, check it<More label="Evidence" className="text-[12px]">{s.answer}</More><button className="btn sm primary" onClick={() => act('/api/tick', { id: s.id })}>Confirm</button><button className="btn sm" onClick={() => act('/api/notyet', { id: s.id })}>Not yet</button></div>}
                <Src i={s} />
                {s.origin === 'owner' && s.state !== 'done' && <div className="mt-1.5"><ItemActions i={s} done={() => { reload(); reloadShell() }} /></div>}
              </div>
              {(s.state === 'open' || s.state === 'doing') && <button className={`btn sm ${s.state === 'doing' ? '' : 'ghost'}`} onClick={() => act('/api/doing', { id: s.id })}>{s.state === 'doing' ? 'Back to to-do' : 'Mark in progress'}</button>}
            </div>
  )

  return (
    <div className="nx-page">
      <div className="nx-in flex flex-wrap items-center gap-4">
        <span className="nx-glyph size-14 text-[22px] font-semibold" style={{ background: c + '1f', color: c, boxShadow: `0 0 36px ${c}33` }}>{th.name[0]}</span>
        <div className="min-w-0">
          <div className="text-[13px] text-mut">{th.area}</div>
          <div className="flex flex-wrap items-center gap-3"><h2 className="text-[30px] font-semibold leading-tight tracking-tight">{th.name}</h2>
            <span className="chip"><span className="size-2 rounded-full" style={{ background: c }} />{STATE_NAME[th.state] ?? th.state}</span>{th.kind && <span className="chip capitalize">{th.kind}</span>}</div>
        </div>
        <div className="ml-auto flex gap-2"><Link className="btn" to={`/map?thread=${encodeURIComponent(th.name)}`}><Network size={14} />Map</Link><button className="btn primary" onClick={() => capture(th.name)}><Plus size={14} />Add</button></div>
      </div>
      <p className="nx-in mt-3 line-clamp-2 max-w-[70ch] text-[14.5px] text-soft" style={{ ['--d' as any]: '40ms' }}>{th.summary}</p>

      <div className="nx-in mt-4 flex flex-wrap items-center gap-2 text-[13px]" style={{ ['--d' as any]: '50ms' }}>
        <span className="text-mut">Needs you:</span>
        {[[open.length, 'to answer', 'questions'], [steps.filter((x: any) => x.state === 'looks_done').length, 'to check', 'steps'], [prop.length, 'to review', 'suggested']].map(([n, l, a]: any) => (
          <a key={l} href={`#${a}`} className="nx-chip" style={{ background: n ? '#fbbf2420' : '#26262b', color: n ? '#fbbf24' : '#8b8b95' }}>{n} {l}</a>
        ))}
      </div>

      <div className="mt-5 grid grid-cols-4 gap-4 max-[1100px]:grid-cols-2">
        <div className="nx-tile nx-in" style={{ ['--d' as any]: '60ms' }}>
          <div className="nx-label"><CheckCheck size={15} />Steps</div>
          <div className="mt-2 flex items-center justify-between"><span className="nx-big">{stepsDone}<span className="text-[18px] font-medium text-mut"> / {steps.length}</span></span>
            <Ring value={steps.length ? stepsDone / steps.length : 0} color={c} size={48} stroke={5} /></div>
          <div className="mt-2 text-[12px] text-mut">{steps.length - stepsDone} still open</div>
        </div>
        <div className="nx-tile nx-in" style={{ ['--d' as any]: '120ms' }}>
          <div className="nx-label"><MessageCircleQuestion size={15} />Claude asks you</div>
          <div className="mt-2 flex items-center justify-between"><span className="nx-big" style={{ color: open.length ? '#fbbf24' : undefined }}>{open.length}</span></div>
          <div className="mt-2 text-[12px] text-mut">{open.length ? 'Answer below' : 'Nothing is blocked'}</div>
        </div>
        <div className="nx-tile nx-in" style={{ ['--d' as any]: '180ms' }}>
          <div className="nx-label"><ScrollText size={15} />Sources</div>
          <div className="mt-2 flex items-center justify-between"><span className="nx-big">{d.sources.length}</span></div>
          <div className="mt-1"><Spark data={spark} color={c} h={34} /></div>
        </div>
        <div className="nx-tile nx-in" style={{ ['--d' as any]: '240ms' }}>
          <div className="nx-label"><Layers size={15} />Last activity</div>
          <div className="mt-2"><span className="nx-big text-[26px]">{last ? agoFmt(last) : 'none'}</span></div>
          <div className="mt-3 text-[12px] text-mut">{last ? dayFmt(last) : ''}</div>
        </div>
      </div>

      <Panel title="Journey" d={280} className="mt-5" hint="One bar per day: how many sources (sessions, notes, artifacts, things typed here) touched this project that day."><Journey d={d} /></Panel>

      {asks.map((i: any) => (
        <section key={i.id} id={`item-${i.id}`} className="nx-panel nx-in mt-5 px-5 py-4" style={hl(i.id)}>
          <div className="flex items-start gap-3">
            <CircleHelp size={18} className="mt-0.5 text-soft" />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 text-[12px] text-mut"><Who i={i} />your question · {dayFmt(i.created)}</div>
              <h3 className="mt-1 text-[16px] font-semibold">{i.title}</h3>
              {i.body && <More label="Detail" className="mt-1 text-[13px] text-soft">{i.body}</More>}
              <div className="mt-2"><ItemActions i={i} done={() => { reload(); reloadShell() }} /></div>
            </div>
          </div>
        </section>
      ))}

      {open.map((i: any) => (
        <section key={i.id} id="questions" className="nx-now nx-in mt-5" style={{ ['--g' as any]: '#fbbf2433', ['--d' as any]: '320ms' }}>
          <div className="relative flex items-start gap-4">
            <span className="nx-glyph size-11" style={{ background: '#fbbf2422', color: '#fbbf24' }}><MessageCircleQuestion size={20} /></span>
            <div className="min-w-0 flex-1">
              <div className="mb-1 flex items-center gap-2 text-[12px] text-mut"><Who i={i} />asks you</div><h3 className="max-w-[40ch] text-[22px] font-semibold leading-[1.2] tracking-tight">{i.title}</h3>
              {i.body && <p className="mt-1.5 line-clamp-2 max-w-[60ch] text-[13.5px] text-soft">{i.body}</p>}
              <Src i={i} />
              <form className="mt-4 flex max-w-[540px] gap-2.5" onSubmit={async (e) => { e.preventDefault(); const r = await post('/api/answer', { id: i.id, text: ans[i.id] ?? '' }); if (r.ok) { setAns({ ...ans, [i.id]: '' }); reload(); reloadShell() } }}>
                <input className="nx-input" maxLength={320} placeholder="Type your answer" value={ans[i.id] ?? ''} onChange={(e) => setAns({ ...ans, [i.id]: e.target.value })} /><button className="nx-btn" type="submit">Answer</button></form>
            </div>
          </div>
        </section>
      ))}

      <div className="mt-5 grid grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)] gap-4 max-[1100px]:grid-cols-1">
        <Panel title="Steps" count={`${openSteps.length} open`} d={360} id="steps">
          {openSteps.length === 0 && <Empty>{steps.length ? 'All steps are done.' : 'No steps.'}</Empty>}
          <Paged resetKey={name} items={openSteps} size={8}>{(pageRows: any[]) => pageRows.map(stepRow)}</Paged>
          <Fold label="Done" count={doneSteps.length}><Paged resetKey={name} items={doneSteps} size={8}>{(pageRows: any[]) => pageRows.map(stepRow)}</Paged></Fold>
        </Panel>

        <div className="flex min-w-0 flex-col gap-4">
          <Panel title="Decisions" count={decisions.length + answered.length} d={400}>
            {decisions.length + answered.length === 0 && <Empty>None kept.</Empty>}
            <Paged resetKey={name} items={decisions} size={5}>{(pageRows: any[]) => pageRows.map((i: any) => <div key={i.id} id={`item-${i.id}`} className="nx-row" style={{ gridTemplateColumns: '32px minmax(0,1fr) auto', cursor: 'default', ...hl(i.id) }}><span className="nx-glyph size-8" style={{ background: '#c084fc1f', color: '#c084fc' }}><Scale size={15} /></span><div className="min-w-0"><div className="text-[14px] font-medium">{i.title}</div>{i.body && <More label="Detail" className="text-[12.5px] text-mut">{i.body}</More>}</div><ItemActions i={i} done={() => { reload(); reloadShell() }} /></div>)}</Paged>
            {answered.map((i: any) => <div key={i.id} className="nx-row" style={{ gridTemplateColumns: '32px minmax(0,1fr)', cursor: 'default' }}><span className="nx-glyph size-8" style={{ background: '#34d3991f', color: '#34d399' }}><CircleHelp size={15} /></span><div className="min-w-0"><div className="text-[14px] font-medium">{i.title}</div><More label={i.origin === 'owner' ? "Claude's answer" : 'Your answer'} className="text-[12.5px]"><span style={{ color: '#34d399' }}>{i.answer}</span></More></div></div>)}
          </Panel>
          <Panel title="Sources" count={d.sources.length} d={440}>
            <div><Paged resetKey={name} items={d.sources} size={8}>{(pageRows: any[]) => pageRows.map((s: any) => <Link key={s.id} to={`/source/${s.id}`} className="nx-row" style={{ gridTemplateColumns: '10px minmax(0,1fr) auto', padding: '10px 18px' }}><span className="size-2 rounded-full" style={{ background: SRC_COLOR[s.kind] }} /><span className="truncate text-[13.5px]">{s.title}</span><span className="text-[12px] text-mut">{dayFmt(s.last_ts)}</span></Link>)}</Paged></div>
          </Panel>
        </div>
      </div>

      {prop.length > 0 && (
        <Panel title="Suggested by Claude" count={prop.length} d={480} className="mt-4" id="suggested" hint="Accept keeps it. Not valid now parks something that was true once. Reject drops it.">
          <Paged resetKey={name} items={[...prop].sort((a: any, b: any) => Number(!!staleOf(a)) - Number(!!staleOf(b)) || (b.source_ts ?? '').localeCompare(a.source_ts ?? ''))} size={6}>{(pageRows: any[]) => pageRows.map((i: any) => {
            const st = staleOf(i)
            return (
              <div key={i.id} id={`item-${i.id}`} className="nx-row" style={{ gridTemplateColumns: '32px minmax(0,1fr) auto', opacity: st ? 0.6 : 1, cursor: 'default', ...hl(i.id) }}>
                <span className="nx-glyph size-8" style={{ background: '#60a5fa1f', color: '#60a5fa' }}><Sparkles size={15} /></span>
                <div className="min-w-0"><div className="text-[14px] font-medium">{i.title}</div>
                  <div className="mt-0.5 text-[12px] text-mut">{dayFmt(i.source_ts ?? i.created)}{st && <> · <span className="text-open" title={st.reason}>{STALE_LABEL[st.kind]}</span></>}</div>{i.body && <More label="Detail" className="text-[12.5px] text-mut">{i.body}</More>}</div>
                <ItemActions i={i} done={() => { reload(); reloadShell() }} />
              </div>
            )
          })}</Paged>
        </Panel>
      )}

      {rej.length > 0 && <section className="nx-panel nx-in mt-4 overflow-hidden"><Fold label="Rejected or not valid now" count={rej.length}><Paged resetKey={name} items={rej} size={5}>{(pageRows: any[]) => pageRows.map((i: any) => <div key={i.id} className="nx-row" style={{ gridTemplateColumns: 'minmax(0,1fr)', cursor: 'default' }}><div><div className="text-[14px] text-mut line-through">{i.title}</div><More label={i.state === 'parked' ? 'Why not valid' : 'Why rejected'} className="text-[12.5px] text-mut">{i.answer || 'No reason given.'}</More></div></div>)}</Paged></Fold></section>}

      <Panel title="Work on it in Claude Code" d={520} className="mt-4">
        <div className="flex flex-wrap items-center gap-2 px-5 pb-4">
          <code className="min-w-0 flex-1 truncate rounded-xl border border-line bg-[#0c0c0e] px-3.5 py-2.5 text-[12px] text-soft">claude "$(backtrail brief --thread '{th.name}')"</code>
          <button className="btn" onClick={() => copy(`claude "$(backtrail brief --thread '${th.name}')"`)}><Copy size={13} />Copy</button>
          {['Review', 'Extend', 'Explore'].map((v) => <button key={v} className="btn" onClick={() => copy(`${v} the Backtrail project "${th.name}". Use backtrail brief, search and show; file proposed items with a source and a verbatim quote (skill/SKILL.md).`)}>{v}</button>)}
        </div>
      </Panel>
    </div>
  )
}
