import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import { Link, NavLink, useLocation, useNavigate } from 'react-router-dom'
import { House, ChevronRight, ListChecks, Network, Plus, ScrollText, Search, Share2, Waypoints } from 'lucide-react'
import { STATE, useApi } from '@/lib/api'
import Capture from '@/components/Capture'

type Ctx = { today: any; reload: () => void; capture: (thread?: string) => void }
const ShellCtx = createContext<Ctx>({ today: null, reload: () => {}, capture: () => {} })
export const useShell = () => useContext(ShellCtx)

const NAV = [
  { to: '/', label: 'Home', icon: House, end: true },
  { to: '/graph', label: 'Graph', icon: Share2, end: false },
]
const MORE = [
  { to: '/items', label: 'Items', icon: ListChecks },
  { to: '/log', label: 'Log', icon: ScrollText },
]

export default function Shell({ children }: { children: ReactNode }) {
  const { data: today, reload } = useApi('/api/today')
  const [cap, setCap] = useState<{ open: boolean; thread?: string }>({ open: false })
  const nav = useNavigate()
  const loc = useLocation()
  useEffect(() => { if (new URLSearchParams(loc.search).get('add')) setCap({ open: true, thread: new URLSearchParams(loc.search).get('thread') ?? undefined }) }, [loc.search])
  const title = ({ map: 'Map', graph: 'Graph', thread: 'Thread', source: 'Source', search: 'Search', items: 'Items', log: 'Log' } as any)[loc.pathname.split('/')[1]] ?? 'Home'
  const live = (today?.threads ?? []).filter((t: any) => t.state === 'moving')
  const others = (today?.threads ?? []).filter((t: any) => t.state !== 'moving' && t.name !== 'Unsorted')
  const [showOthers, setShowOthers] = useState(false)
  const toggleOthers = () => setShowOthers((v) => !v)
  // a quiet dot, not a count: something fresh (under a week old) is waiting in the Home focus card
  const fresh = (today?.looks?.length ?? 0) + (today?.waiting ?? []).filter((q: any) => Date.now() - (Date.parse(q.created ?? '') || 0) < 7 * 864e5).length

  return (
    <ShellCtx.Provider value={{ today, reload, capture: (thread) => setCap({ open: true, thread }) }}>
      <div className="flex h-full bg-side">
        <aside className="hidden w-60 shrink-0 flex-col gap-1 overflow-y-auto px-3 py-4 md:flex">
          <Link to="/" className="mb-3 flex items-center gap-2.5 px-2 py-1.5">
            <span className="grid size-8 place-items-center rounded-lg bg-[#111] shadow-[0_2px_14px_rgba(52,211,153,.4)]"><Waypoints size={17} className="text-accent" /></span>
            <span className="text-[15px] font-semibold tracking-tight">Backtrail</span>
          </Link>
          {NAV.map(({ to, label, icon: Icon, end }) => (
            <NavLink key={to} to={to} end={end} className={({ isActive }) => `row-hover flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13.5px] ${isActive ? 'bg-white/[.07] font-medium text-ink shadow-[inset_0_1px_0_rgba(255,255,255,.05)]' : 'text-soft'}`}>
              <Icon size={16} />{label}
              {to === '/' && fresh > 0 && <span className="ml-auto size-2 rounded-full bg-accent" title="Something is waiting in the focus card" />}
            </NavLink>
          ))}
          <div className="mt-5 px-2.5 pb-1 text-[12px] text-mut" title="Threads whose state is In development. The yellow number is how many questions in it wait for your answer.">Threads</div>
          {live.map((t: any) => (
            <Link key={t.name} to={`/thread/${encodeURIComponent(t.name)}`} className="row-hover flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] text-soft">
              <span className="size-2 rounded-full" style={{ background: STATE.moving.color }} />
              <span className="truncate">{t.name}</span>
              {t.open_q > 0 && <span className="ml-auto text-[11px] text-open">{t.open_q}</span>}
            </Link>
          ))}
          {others.length > 0 && (
            <>
              <button type="button" onClick={toggleOthers} className="mt-4 flex items-center gap-1.5 px-2.5 pb-1 text-left text-[12px] text-mut hover:text-ink" title="Built, parked, dropped and open-idea threads">
                <ChevronRight size={12} className={`transition ${showOthers ? 'rotate-90' : ''}`} />Other threads <span className="tabular-nums">({others.length})</span>
              </button>
              {showOthers && <div>{['built', 'parked', 'open', 'dropped'].map((st) => {
                const g = others.filter((t: any) => t.state === st)
                return g.length === 0 ? null : (
                  <div key={st}>
                    <div className="px-2.5 pb-0.5 pt-2.5 text-[11px] uppercase tracking-wide text-mut">{STATE[st]?.label ?? st} <span className="tabular-nums">{g.length}</span></div>
                    {g.map((t: any) => (
                      <Link key={t.name} to={`/thread/${encodeURIComponent(t.name)}`} className="row-hover flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] text-soft">
                        <span className="size-2 shrink-0 rounded-full" style={{ background: STATE[st]?.color ?? '#52525b' }} />
                        <span className="truncate">{t.name}</span>
                      </Link>
                    ))}
                  </div>
                )
              })}</div>}
            </>
          )}
          <div className="mt-auto pt-4">
            {MORE.map(({ to, label, icon: Icon }) => (
              <NavLink key={to} to={to} className={({ isActive }) => `row-hover flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] ${isActive ? 'bg-white/[.07] font-medium text-ink' : 'text-soft'}`}><Icon size={15} />{label}</NavLink>
            ))}
          </div>
          <div className="px-2.5 pt-3 text-[11.5px] text-mut">
            {today ? `${today.totals.sessions} sessions, ${today.totals.notes} notes, ${today.totals.artifacts} artifacts` : ''}
          </div>
        </aside>

        <main className="m-2 ml-0 flex min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border border-line bg-bg max-md:m-0 max-md:rounded-none">
          <header className="flex h-14 shrink-0 items-center gap-3 border-b border-line px-5">
            <h1 className="text-[15px] font-semibold">{title}</h1>
            <div className="relative ml-4 max-w-sm flex-1">
              <Search size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-mut" />
              <input className="input !pl-9" placeholder="Search everything" onKeyDown={(e) => { if (e.key === 'Enter') { const q = (e.target as HTMLInputElement).value; if (q) nav(`/search?q=${encodeURIComponent(q)}`); else nav('/') } }} />
            </div>
            {!loc.pathname.startsWith('/thread') && <button className="btn primary ml-auto" onClick={() => setCap({ open: true })}><Plus size={15} />Add</button>}
          </header>
          <div className="min-h-0 flex-1 overflow-auto">{children}</div>
        </main>
      </div>
      {cap.open && <Capture thread={cap.thread} onClose={() => setCap({ open: false })} onDone={reload} />}
    </ShellCtx.Provider>
  )
}
