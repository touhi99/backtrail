import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { Check, RotateCcw } from 'lucide-react'
import { dayFmt, useApi } from '@/lib/api'
import ItemActions from '@/components/ItemActions'
import type { Item, SourceRaw, ThreadClaim, ThreadDetail } from '@/lib/types'

type Session = { d: string; t: string; id: number }
type Made = { d: string; t: string; id: number; kind: string; n: number }
type StoryEvent = ThreadClaim & { d: string; t: string }
type SettledEntry = { t: string; q: string | null; who: string; ok: boolean; d: string | null; item?: Item; answer?: boolean }

const DAY = 864e5
const ts = (d: string) => Date.UTC(+d.slice(0, 4), +d.slice(5, 7) - 1, +d.slice(8, 10))
const iso = (t: number) => new Date(t).toISOString().slice(0, 10)
const isYou = (who?: string) => who === 'owner' || who === 'you'
export const you = (t?: string) => !t ? '' : t
  .replace(/\bthe owner's\b/g, 'your').replace(/\bOwner's\b/g, 'Your').replace(/\bowner's\b/g, 'your')
  .replace(/\bThe owner\b/g, 'You').replace(/\bthe owner\b/g, 'you').replace(/\bOwner\b/g, 'You').replace(/\bowner\b/g, 'you')
  .replace(/\bYou (wants|needs|judges|counts|treats|thinks|prefers|likes)\b/g, (_, v) => 'You ' + v.slice(0, -1))
  .replace(/\bYou does\b/g, 'You do')
const wclip = (s: string, n: number) => s.length <= n ? s : s.slice(0, s.lastIndexOf(' ', n)).replace(/[,;:]$/, '') + '…'
const words = (s: string) => new Set(s.toLowerCase().split(/\W+/).filter((w) => w.length > 2))
const similar = (a: string, b: string) => { const A = words(a), B = words(b); const i = [...A].filter((w) => B.has(w)).length; return i / Math.max(1, Math.min(A.size, B.size)) }
const DONE = (s: string) => s === 'done' || s === 'looks_done'

function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null)
  const [w, setW] = useState(0)
  useLayoutEffect(() => {
    if (!ref.current) return
    const ro = new ResizeObserver(([e]) => setW(e.contentRect.width))
    ro.observe(ref.current)
    return () => ro.disconnect()
  }, [])
  return [ref, w] as const
}

export type Story = ReturnType<typeof buildStory>

export function buildStory(d: ThreadDetail) {
  const live = d.items.filter((i) => !['proposed', 'rejected', 'parked'].includes(i.state))
  const sessions: Session[] = d.sources.filter((s) => s.kind === 'session' && s.first_ts).map((s) => ({ d: s.first_ts.slice(0, 10), t: s.title ?? '', id: s.id }))
  const made: Made[] = d.sources.filter((s) => (s.kind === 'artifact' || s.kind === 'commit') && (s.first_ts || s.last_ts))
    .map((s) => ({ d: (s.kind === 'commit' ? s.last_ts : s.first_ts).slice(0, 10), t: s.title ?? '', id: s.id, kind: s.kind, n: s.nchunks ?? 0 }))
    .sort((a, b) => a.d.localeCompare(b.d))
  const steps = live.filter((i) => i.type === 'step')
  const groups = new Map<string, Item[]>()
  for (const s of steps) {
    const m = s.title.match(/^([A-Z]+)-(\d+)/), n = /^\d+ ·/.test(s.title)
    const key = m ? m[1] : n ? '#' : 'other'
    if (!groups.has(key)) groups.set(key, [])
    groups.get(key)!.push(s)
  }
  const num = (s: Item) => +(s.title.match(/(\d+)/)?.[1] ?? 0)
  const milestones = [...groups.entries()].map(([k, items]) => {
    items.sort((a, b) => (k === 'other' ? 0 : num(a) - num(b)))
    const ds = items.map((x) => (x.updated ?? x.created ?? '').slice(0, 10)).filter(Boolean).sort()
    const name = k === 'other' ? 'Other steps' : k === '#' ? 'Numbered plan' : `${k}-${num(items[0])} to ${k}-${num(items[items.length - 1])}`
    return { name, items, done: items.filter((x) => x.state === 'done').length, check: items.filter((x) => x.state === 'looks_done').length, d0: ds[0], d1: ds[ds.length - 1] }
  }).sort((a, b) => (a.name === 'Other steps' ? 1 : b.name === 'Other steps' ? -1 : (a.d0 ?? '').localeCompare(b.d0 ?? '')))
  const claims = d.claims ?? []
  const events: StoryEvent[] = claims.filter((c) => (c.type === 'state' || c.type === 'rationale') && c.d).map((c) => ({ ...c, d: c.d ?? '', t: you(c.text) }))
  const turns = claims.filter((c) => c.type === 'decision' && c.status === 'superseded' && c.newer && similar(c.text, c.newer) < 0.6)
    .map((c) => ({ d: c.d ?? '', t: you(c.text), n: you(c.newer ?? '') }))
  const seen = new Set<string>()
  const settled: SettledEntry[] = []
  const add = (x: SettledEntry) => { const k = x.t.toLowerCase(); if (!seen.has(k)) { seen.add(k); settled.push(x) } }
  live.filter((i) => i.type === 'decision' && !(i.stale ?? '').startsWith('superseded')).forEach((i) => add({ t: i.title, q: i.quote, who: i.origin === 'owner' ? 'owner' : 'claude', ok: true, d: (i.created ?? '').slice(0, 10), item: i }))
  live.filter((i) => i.type === 'question' && i.state !== 'open' && i.answer).forEach((i) => add({ t: i.title, q: i.answer, who: i.origin === 'owner' ? 'claude' : 'owner', ok: true, d: (i.created ?? '').slice(0, 10), answer: true }))
  claims.filter((c) => c.type === 'decision' && c.status === 'active').forEach((c) => add({ t: you(c.text), q: c.q, who: c.speaker, ok: true, d: c.d }))
  claims.filter((c) => c.type === 'decision' && c.status === 'proposed').sort((a, b) => Number(!isYou(a.speaker)) - Number(!isYou(b.speaker)) || (b.d ?? '').localeCompare(a.d ?? ''))
    .forEach((c) => add({ t: you(c.text), q: c.q, who: c.speaker, ok: false, d: c.d }))
  const openSteps = steps.filter((s) => !DONE(s.state))
  const dates = [...sessions.map((s) => s.d), ...made.filter((m) => m.kind === 'artifact').map((m) => m.d), ...events.map((e) => e.d)].filter((x) => x > '2000').sort()
  return { sessions, made, steps, milestones, events, turns, settled, openSteps, first: dates[0], last: dates[dates.length - 1] }
}

export function Thumb({ id, title, sub, href, about }: { id: number; title: string; sub?: string; href?: string; about?: string }) {
  const { data } = useApi<SourceRaw>(`/api/source_raw?id=${id}`)
  const [ref, w] = useWidth<HTMLDivElement>()
  const body = (
    <>
      <div ref={ref} className="relative overflow-hidden rounded-xl border border-line bg-[#141416] transition group-hover:border-[#3a3a42]" style={{ aspectRatio: '16 / 10' }}>
        {data?.text && w > 0 && <iframe title={title} srcDoc={data.text} sandbox="allow-scripts" tabIndex={-1} className="pointer-events-none absolute left-0 top-0 origin-top-left border-0" style={{ width: 1280, height: 800, transform: `scale(${w / 1280})`, colorScheme: 'dark' }} />}
      </div>
      <div className="text-[12.5px] text-mut"><span className="font-medium text-ink">{title}</span>{sub ? `, ${sub}` : ''}{about ? <div className="mt-1 line-clamp-2 text-[12px]">{about}</div> : null}</div>
    </>
  )
  const cls = 'group flex min-w-0 flex-col gap-2'
  return href ? <a href={href} target="_blank" rel="noreferrer" className={cls}>{body}</a> : <Link to={`/source/${id}`} className={cls}>{body}</Link>
}

function Mini({ n, label, color, children }: { n: ReactNode; label: string; color?: string; children?: ReactNode }) {
  return <div className="flex items-center gap-2.5 rounded-2xl bg-white/[.035] px-3.5 py-2.5">{children}<div><div className="text-[22px] font-semibold leading-none tabular-nums" style={{ color }}>{n}</div><div className="mt-1 text-[12px] text-mut">{label}</div></div></div>
}

export function ProgressRing({ done, check, total, size = 40, color }: { done: number; check: number; total: number; size?: number; color: string }) {
  const r = size / 2 - 4, C = 2 * Math.PI * r, a = total ? done / total : 0, b = total ? (done + check) / total : 0
  return (
    <svg width={size} height={size} className="shrink-0" style={{ transform: 'rotate(-90deg)' }}>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="#26262b" strokeWidth={4} />
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="#facc15" strokeWidth={4} strokeLinecap="round" strokeDasharray={`${b * C} ${C}`} />
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={color} strokeWidth={4} strokeLinecap="round" strokeDasharray={`${a * C} ${C}`} />
    </svg>
  )
}

export function StoryHero({ s, state, color }: { s: Story; state: string; color: string }) {
  const art = [...s.made].reverse().find((m) => m.kind === 'artifact' && m.n > 1)
  const full = s.milestones.filter((m) => m.items.every((x) => DONE(x.state)))
  const part = s.milestones.filter((m) => !full.includes(m) && m.done + m.check > 0)
  const wins = s.events.filter((e) => !isYou(e.speaker) && /\b(merged|live|released|now run|works again|no longer|shipped|published)\b/i.test(e.t))
  const why = [...s.events].reverse().find((e) => isYou(e.speaker) && e.type === 'rationale') ?? [...s.events].reverse().find((e) => /false|fail|drop|stop/i.test(e.t))
  const ended = state === 'dropped' || state === 'parked'
  const days = s.first && s.last ? (ts(s.last) - ts(s.first)) / DAY + 1 : 0
  return (
    <section className="nx-in mt-5 grid grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)] gap-6 rounded-3xl border p-6 max-[1000px]:grid-cols-1" style={{ '--d': '60ms', borderColor: color + '40', background: `radial-gradient(120% 140% at 0% 0%, ${color}1c 0%, #141416 55%)` }}>
      <div className="min-w-0">
        <div className="rounded-2xl border p-4" style={{ borderColor: color + '38', background: color + '12' }}>
          {ended && why ? <>
            <div className="text-[12.5px] font-semibold" style={{ color }}>{state === 'dropped' ? 'Why it ended' : 'Why it is parked'}</div>
            <div className="mt-1 text-[21px] font-semibold leading-snug" style={{ textWrap: 'balance' }}>{why.t}</div>
            {why.q && <div className="mt-1.5 text-[13px] italic text-mut">“{wclip(why.q, 160)}”, {dayFmt(why.d)}</div>}
          </> : <>
            <div className="text-[12.5px] font-semibold" style={{ color }}>What exists now</div>
            <ul className="mt-2 flex flex-col gap-2">
              {full.map((m) => <li key={m.name} className="flex items-baseline gap-2.5 text-[15.5px] font-semibold"><Check size={15} style={{ color }} className="shrink-0 translate-y-0.5" />{m.name}<span className="ml-auto whitespace-nowrap text-[12.5px] font-normal text-mut">all {m.items.length} steps</span></li>)}
              {part.map((m) => <li key={m.name} className="flex items-baseline gap-2.5 text-[15.5px] font-semibold"><Check size={15} style={{ color }} className="shrink-0 translate-y-0.5" />{m.name}<span className="ml-auto whitespace-nowrap text-[12.5px] font-normal text-mut">{m.done + m.check} of {m.items.length} steps</span></li>)}
              {wins.slice(-3).map((e) => <li key={e.id} className="flex items-baseline gap-2.5 text-[15px] font-semibold"><Check size={15} style={{ color }} className="shrink-0 translate-y-0.5" />{e.t}<span className="ml-auto whitespace-nowrap text-[12.5px] font-normal text-mut">{dayFmt(e.d)}</span></li>)}
              {!full.length && !part.length && !wins.length && <li className="text-[14px] text-mut">Nothing is recorded as finished yet.</li>}
            </ul>
          </>}
        </div>
        {s.openSteps.length > 0 && <a href="#steps" className="mt-3 block text-[13px] text-mut hover:text-ink">Next up: <span className="font-medium text-ink">{s.openSteps[0].title}</span>{s.openSteps.length > 1 ? `, and ${s.openSteps.length - 1} more open` : ''}</a>}
        <div className="mt-4 grid grid-cols-4 gap-2.5 max-[700px]:grid-cols-2">
          <Mini n={s.sessions.length} label={days ? `sessions in ${days} days` : 'sessions'} />
          <Mini n={s.made.length} label="things made" color="#c084fc" />
          {s.steps.length > 0 && <Mini n={<>{s.steps.filter((x) => DONE(x.state)).length}<span className="text-[13px] text-mut">/{s.steps.length}</span></>} label="steps done"><ProgressRing size={34} color={color} total={s.steps.length} done={s.steps.filter((x) => x.state === 'done').length} check={s.steps.filter((x) => x.state === 'looks_done').length} /></Mini>}
          <Mini n={s.settled.length} label="decisions" color="#60a5fa" />
        </div>
      </div>
      {art ? <Thumb id={art.id} title={art.t} sub={`latest thing made, ${dayFmt(art.d)}`} />
        : <div className="grid place-items-center rounded-2xl border border-dashed border-line text-[13px] text-mut">No artifact was made for this thread.</div>}
    </section>
  )
}

export function JourneyRail({ s, color }: { s: Story; color: string }) {
  const [ref, cw] = useWidth<HTMLDivElement>()
  const [tip, setTip] = useState<{ x: number; y: number; h: ReactNode } | null>(null)
  const W = Math.max(860, cw), L = 24, R = 24, PW = W - L - R
  const first = s.first, last = s.last
  const calc = useMemo(() => {
    if (!first || !last) return null
    const span = Math.max(1, (ts(last) - ts(first)) / DAY)
    const x = (d: string) => L + ((ts(d < first ? first : d > last ? last : d) - ts(first)) / DAY) / span * PW
    const per: Record<string, Session[]> = {}
    s.sessions.forEach((q) => (per[q.d] ??= []).push(q))
    const mx = Math.max(1, ...Object.values(per).map((v) => v.length))
    const score = (e: StoryEvent) => (isYou(e.speaker) ? 2 : 0) + (e.type === 'rationale' ? 1 : 0) + (/\b(live|released|merged|uninstalled|stopped|dropped|removed|works|now run)\b/i.test(e.t) ? 2 : 0)
    const byScore = [...s.events].sort((a, b) => score(b) - score(a))
    const wk = (e: StoryEvent) => Math.floor((ts(e.d) - ts(first)) / DAY / 7)
    const best = new Map<number, StoryEvent>(); byScore.forEach((e) => { if (!best.has(wk(e))) best.set(wk(e), e) })
    const cand = [...best.values(), ...byScore.filter((e) => ![...best.values()].includes(e))]
    const rows: number[][] = [[], []], pins: (StoryEvent & { r: number })[] = []
    for (const e of cand) { if (pins.length >= 6) break; const xe = x(e.d); for (let r = 0; r < 2; r++) if (rows[r].every((q) => Math.abs(q - xe) > 216)) { rows[r].push(xe); pins.push({ ...e, r }); break } }
    return { span, x, per, mx, pins }
  }, [s, first, last, PW])
  if (!calc) return null
  const { span, x, per, mx, pins } = calc
  const AX = 270, MSY = AX + 40, H = MSY + s.milestones.length * 26 + 8
  const pts: [number, number][] = []
  for (let k = 0; k <= span; k++) { const d = iso(ts(first!) + k * DAY); pts.push([x(d), AX - ((per[d]?.length ?? 0) / mx) * 64]) }
  let path = `M${pts[0][0]} ${AX} L${pts[0][0]} ${pts[0][1]}`
  for (let i = 1; i < pts.length; i++) { const [a, b] = pts[i], [pa, pb] = pts[i - 1], m = (pa + a) / 2; path += ` C${m} ${pb}, ${m} ${b}, ${a} ${b}` }
  path += ` L${pts[pts.length - 1][0]} ${AX} Z`
  const tick = span > 21 ? 7 : span > 8 ? 2 : 1
  const show = (e: React.MouseEvent, h: ReactNode) => { const r = ref.current!.getBoundingClientRect(); setTip({ x: e.clientX - r.left + ref.current!.scrollLeft, y: e.clientY - r.top, h }) }
  return (
    <section className="nx-panel nx-in mt-5 px-5 pb-4 pt-4" style={{ '--d': '120ms' }}>
      <div className="flex items-baseline gap-3"><h3 className="text-[15px] font-semibold">Journey</h3><span className="text-[12.5px] text-mut">hover a mark for detail</span></div>
      <div ref={ref} className="relative overflow-x-auto" onMouseLeave={() => setTip(null)}>
        <div className="relative" style={{ width: W, height: H }}>
          <svg width={W} height={H} className="absolute inset-0">
            <defs><linearGradient id="jr" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor={color} stopOpacity=".5" /><stop offset="1" stopColor={color} stopOpacity=".04" /></linearGradient></defs>
            <path d={path} fill="url(#jr)" />
            <line x1={L} x2={W - R} y1={AX} y2={AX} stroke="#2a2a30" strokeWidth={1.5} />
            {Array.from({ length: Math.floor(span / tick) + 1 }, (_, k) => iso(ts(first!) + k * tick * DAY)).map((d) => <text key={d} x={x(d)} y={AX + 17} fill="#6b6b75" fontSize={11} textAnchor="middle">{dayFmt(d)}</text>)}
            {Object.entries(per).map(([d, v]) => <rect key={d} x={x(d) - 9} y={AX - 70} width={18} height={70} fill="transparent" onMouseMove={(e) => show(e, <><div className="font-semibold">{dayFmt(d)}, {v.length} session{v.length > 1 ? 's' : ''}</div><ul className="mt-1 list-disc pl-4 text-mut">{v.map((q) => <li key={q.id}>{q.t}</li>)}</ul></>)} />)}
            {pins.map((e) => { const xe = x(e.d), c = isYou(e.speaker) ? '#34d399' : '#8b8b95'; return <g key={e.id}><line x1={xe} x2={xe} y1={(e.r ? 100 : 0) + 64} y2={AX - 4} stroke={c} strokeOpacity={0.5} strokeDasharray="2 3" /><circle cx={xe} cy={AX} r={4} fill={c} /></g> })}
            {s.events.filter((e) => !pins.some((p) => p.id === e.id)).map((e) => <circle key={e.id} cx={x(e.d)} cy={AX} r={3} fill={isYou(e.speaker) ? '#34d399' : '#8b8b95'} fillOpacity={0.75} onMouseMove={(ev) => show(ev, <><div className="font-semibold">{dayFmt(e.d)}, {isYou(e.speaker) ? 'you' : 'Claude'}</div><div className="mt-1 text-mut">{e.t}</div></>)} />)}
            {s.made.filter((m) => m.kind === 'artifact').map((m) => { const xm = x(m.d); return <rect key={m.id} x={xm - 5} y={AX - 5} width={10} height={10} transform={`rotate(45 ${xm} ${AX})`} fill="#c084fc" stroke="#141416" strokeWidth={2} onMouseMove={(e) => show(e, <><div className="font-semibold">Made, {dayFmt(m.d)}</div><div className="mt-1 text-mut">{m.t}</div></>)} /> })}
            {s.milestones.map((m, i) => { const y = MSY + i * 26, a = x(m.d0 ?? first!) - 6, b = Math.max(x(m.d1 ?? first!) + 6, a + 14), f = (m.done + m.check) / m.items.length, lx = b + 8 > W - 200 ? a - 8 : b + 8
              return <g key={m.name}><rect x={a} y={y} width={b - a} height={16} rx={8} fill={color} fillOpacity={0.14} stroke={color} strokeOpacity={0.4} /><rect x={a} y={y} width={(b - a) * f} height={16} rx={8} fill={color} fillOpacity={0.5} />
                <text x={lx} y={y + 12} fill="#e4e4e7" fontSize={12} fontWeight={600} textAnchor={lx < a ? 'end' : 'start'}>{m.name} <tspan fill="#8b8b95" fontWeight={400}>{m.done + m.check}/{m.items.length}</tspan></text></g> })}
          </svg>
          {pins.map((e) => { const xe = x(e.d), you_ = isYou(e.speaker); return (
            <div key={e.id} title={e.q ?? ''} className="absolute w-[200px] rounded-xl border px-2.5 py-2 text-[12px] leading-snug" style={{ left: Math.min(Math.max(xe - 100, 0), W - 202), top: e.r ? 100 : 0, background: '#1a1a1e', borderColor: you_ ? '#34d39966' : '#2a2a30' }}>
              <div className="mb-0.5 text-[11px]" style={{ color: you_ ? '#34d399' : '#6b6b75' }}>{dayFmt(e.d)}, {you_ ? 'you' : 'Claude'}</div>{wclip(e.t, 80)}
            </div>) })}
          {tip && <div className="pointer-events-none absolute z-10 max-w-[340px] rounded-xl border border-line bg-[#1a1a1e] px-3 py-2 text-[12.5px] shadow-xl" style={{ left: Math.min(tip.x + 12, W - 350), top: tip.y + 12 }}>{tip.h}</div>}
        </div>
      </div>
      <div className="mt-2 flex flex-wrap gap-4 text-[12px] text-mut">
        <span className="flex items-center gap-1.5"><span className="size-2.5 rounded-sm" style={{ background: color, opacity: 0.6 }} />Sessions per day</span>
        <span className="flex items-center gap-1.5"><span className="size-2 rotate-45 bg-[#c084fc]" />Made</span>
        <span className="flex items-center gap-1.5"><span className="size-2 rounded-full bg-[#34d399]" />Something you said</span>
        <span className="flex items-center gap-1.5"><span className="size-2 rounded-full bg-[#8b8b95]" />Something Claude found</span>
      </div>
    </section>
  )
}

const DOT: Record<string, string> = { done: '#34d399', looks_done: '#facc15' }

export function Milestones({ s, color }: { s: Story; color: string }) {
  const [openM, setOpenM] = useState<Record<string, boolean>>({})
  if (!s.milestones.length && !s.turns.length) return null
  const order: Record<string, number> = { doing: 0, open: 1, looks_done: 2, done: 3 }
  return (
    <div className={`mt-5 grid gap-4 ${s.turns.length && s.milestones.length ? 'grid-cols-2 max-[1000px]:grid-cols-1' : ''}`}>
      {s.milestones.length > 0 && <section className="nx-panel nx-in p-5" style={{ '--d': '180ms' }}>
        <h3 className="mb-3 text-[15px] font-semibold">Milestones</h3>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
          {s.milestones.map((m) => { const its = [...m.items].sort((a, b) => (order[a.state] ?? 4) - (order[b.state] ?? 4)); const all = openM[m.name]
            return (
              <div key={m.name} className="flex flex-col gap-2.5 rounded-2xl bg-white/[.035] p-3.5">
                <div className="flex items-center gap-3"><ProgressRing done={m.done} check={m.check} total={m.items.length} size={44} color={color} />
                  <div className="min-w-0"><div className="text-[15px] font-semibold">{m.name}</div><div className="text-[12px] text-mut">{m.done + m.check} of {m.items.length} done{m.check ? `, ${m.check} for you to check` : ''}</div></div></div>
                <ul className="flex flex-col gap-1">{(all ? its : its.slice(0, 4)).map((x) => <li key={x.id} className="flex gap-2 text-[12.5px] text-mut"><span className="mt-[6px] size-[7px] shrink-0 rounded-full" style={DOT[x.state] ? { background: DOT[x.state] } : { border: '1.5px solid #8b8b95' }} /><Link to={`?item=${x.id}`} className="hover:text-ink">{x.title}</Link></li>)}</ul>
                {its.length > 4 && <button type="button" className="text-left text-[12.5px] font-medium text-accent hover:underline" onClick={() => setOpenM({ ...openM, [m.name]: !all })}>{all ? 'Show fewer' : `Show ${its.length - 4} more`}</button>}
              </div>) })}
        </div>
      </section>}
      {s.turns.length > 0 && <section className="nx-panel nx-in p-5" style={{ '--d': '200ms' }}>
        <h3 className="mb-3 text-[15px] font-semibold">Changed course</h3>
        <div className="flex flex-col gap-2.5">{s.turns.slice(-6).reverse().map((t, i) => (
          <div key={i} className="grid grid-cols-[28px_minmax(0,1fr)] gap-3 rounded-xl bg-white/[.035] p-3"><span className="grid size-7 place-items-center rounded-lg bg-[#fbbf24]/15 text-[#fbbf24]"><RotateCcw size={14} /></span>
            <div><div className="text-[13px] text-mut line-through decoration-[#fb7185]/60">{t.t}</div><div className="mt-0.5 text-[13.5px]">{t.n}</div><div className="mt-1 text-[11.5px] text-mut">{dayFmt(t.d)}</div></div></div>))}</div>
      </section>}
    </div>
  )
}

export function Settled({ s, done }: { s: Story; done: () => void }) {
  const [all, setAll] = useState(false)
  useEffect(() => setAll(false), [s])
  if (!s.settled.length) return null
  const ok = s.settled.filter((x) => x.ok).length
  return (
    <section className="nx-panel nx-in mt-5 p-5" id="decisions">
      <h3 className="mb-3 flex items-baseline gap-2.5 text-[15px] font-semibold">What you settled<span className="text-[12.5px] font-normal text-mut">{s.settled.length} decisions, {ok} confirmed</span></h3>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(270px,1fr))] gap-3">
        {(all ? s.settled : s.settled.slice(0, 8)).map((x, i) => (
          <div key={i} className="flex flex-col gap-2 rounded-2xl border bg-white/[.035] p-3.5" style={{ borderColor: x.ok ? '#34d39944' : 'transparent' }}>
            <div className="text-[14.5px] font-semibold leading-snug" style={{ textWrap: 'balance' }}>{x.t}</div>
            {x.q && <div className="border-l-2 border-line pl-2.5 text-[12.5px] italic text-mut [overflow-wrap:anywhere]">{x.answer ? x.q : `“${wclip(x.q, 150)}”`}</div>}
            <div className="mt-auto flex flex-wrap items-center gap-2 text-[11.5px] text-mut">
              <span className="rounded-md px-1.5 py-px font-semibold" style={isYou(x.who) ? { background: '#34d39922', color: '#34d399' } : { background: '#60a5fa22', color: '#60a5fa' }}>{x.answer ? (isYou(x.who) ? 'You answered' : 'Claude answered') : isYou(x.who) ? 'You said' : 'Claude'}</span>
              {x.ok && <span className="text-accent">✓ confirmed</span>}<span>{dayFmt(x.d)}</span>
              {x.item && <span className="ml-auto"><ItemActions i={x.item} done={done} /></span>}
            </div>
          </div>))}
      </div>
      {s.settled.length > 8 && <button type="button" className="mt-3 text-[12.5px] font-medium text-accent hover:underline" onClick={() => setAll(!all)}>{all ? 'Show fewer' : `Show all ${s.settled.length}`}</button>}
    </section>
  )
}

export function Made({ s }: { s: Story }) {
  if (!s.made.length) return null
  return (
    <section className="nx-panel nx-in mt-5 p-5">
      <h3 className="mb-3 flex items-baseline gap-2.5 text-[15px] font-semibold">Made<span className="text-[12.5px] font-normal text-mut">{s.made.length}</span></h3>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3.5">
        {[...s.made].reverse().map((m) => m.kind === 'artifact' && m.n > 1 ? <Thumb key={m.id} id={m.id} title={m.t} sub={dayFmt(m.d)} />
          : <Link key={m.id} to={`/source/${m.id}`} className="flex min-w-0 flex-col gap-2"><div className="grid place-items-center rounded-xl border border-line bg-[#141416] text-[13px] text-mut" style={{ aspectRatio: '16 / 10' }}>{m.kind === 'commit' ? 'Commits' : 'No preview'}</div><div className="text-[12.5px] text-mut"><span className="font-medium text-ink">{m.t}</span>, {dayFmt(m.d)}</div></Link>)}
      </div>
    </section>
  )
}
