import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import { Link, NavLink, useLocation, useNavigate } from 'react-router-dom'
import { House, ChevronRight, ListChecks, MessageSquareWarning, Plus, ScrollText, Search as SearchIcon, Share2, X } from 'lucide-react'
import { STATE, useApi } from '@/lib/api'
import Capture from '@/components/Capture'
import Search from '@/pages/Search'
import Items from '@/pages/Items'
import Log from '@/pages/Log'
import type { KgSuggest, ThreadStat, Today } from '@/lib/types'

function TrailMark({ size = 17, className = '' }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" className={className}>
      <circle cx="5" cy="5" r="2.5" /><path d="M7.5 5H13a3.5 3.5 0 0 1 0 7h-2a3.5 3.5 0 0 0 0 7h5.5" /><circle cx="19" cy="19" r="2.5" />
    </svg>
  )
}

type Ctx = { today: Today | null; reload: () => void; capture: (thread?: string) => void; search: (q: string, thread?: string) => void }
const ShellCtx = createContext<Ctx>({ today: null, reload: () => {}, capture: () => {}, search: () => {} })
export const useShell = () => useContext(ShellCtx)

const TITLE: Record<string, string> = { map: 'Map', graph: 'Graph', corrections: 'Corrections', thread: 'Thread', source: 'Source' }

const NAV = [
  { to: '/', label: 'Home', icon: House, end: true },
  { to: '/graph', label: 'Graph', icon: Share2, end: false },
  { to: '/corrections', label: 'Corrections', icon: MessageSquareWarning, end: false },
]
type Sheet = { kind: 'search'; q: string; thread: string } | { kind: 'items' } | { kind: 'log' }

function Modal({ onClose, children }: { onClose: () => void; children: ReactNode }) {
  useEffect(() => { const h = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }; window.addEventListener('keydown', h); return () => window.removeEventListener('keydown', h) }, [onClose])
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/60 backdrop-blur-sm" onMouseDown={onClose}>
      <div className="relative flex max-h-[86vh] w-[min(1000px,94vw)] flex-col overflow-hidden rounded-[22px] border border-line2 bg-bg shadow-[0_24px_60px_rgba(0,0,0,.55)]" onMouseDown={(e) => e.stopPropagation()}>
        <button type="button" className="btn ghost sm absolute right-3 top-3 z-10" onClick={onClose} title="Close (Esc)"><X size={14} /></button>
        <div className="overflow-auto p-6">{children}</div>
      </div>
    </div>
  )
}

export default function Shell({ children }: { children: ReactNode }) {
  const { data: today, reload } = useApi<Today>('/api/today')
  const { data: sug, reload: reloadSug } = useApi<KgSuggest>('/api/kg/suggest')
  useEffect(() => { window.addEventListener('bt-suggest', reloadSug); return () => window.removeEventListener('bt-suggest', reloadSug) }, [reloadSug])
  const toReview = sug ? sug.entities.length + sug.aliases.length : 0
  const [cap, setCap] = useState<{ open: boolean; thread?: string }>({ open: false })
  const [sheet, setSheet] = useState<Sheet | null>(null)
  const nav = useNavigate()
  const loc = useLocation()
  useEffect(() => { if (new URLSearchParams(loc.search).get('add')) setCap({ open: true, thread: new URLSearchParams(loc.search).get('thread') ?? undefined }) }, [loc.search])
  const title = TITLE[loc.pathname.split('/')[1]] ?? 'Home'
  const scope = loc.pathname.startsWith('/thread/') ? decodeURIComponent(loc.pathname.slice(8)) : ''
  useEffect(() => { setSheet(null) }, [loc.pathname, loc.search])
  const live = (today?.threads ?? []).filter((t) => t.state === 'moving')
  const nested = (t: ThreadStat, list: ThreadStat[]) => !!t.parent && list.some((p) => p.name === t.parent)
  const others = (today?.threads ?? []).filter((t) => t.state !== 'moving' && t.name !== 'Unsorted')
  const [showOthers, setShowOthers] = useState(false)
  const toggleOthers = () => setShowOthers((v) => !v)

  return (
    <ShellCtx.Provider value={{ today, reload, capture: (thread) => setCap({ open: true, thread }), search: (q, thread = '') => setSheet({ kind: 'search', q, thread }) }}>
      <div className="flex h-full bg-side">
        <aside className="hidden w-60 shrink-0 flex-col gap-1 overflow-y-auto px-3 py-4 md:flex">
          <Link to="/" className="mb-3 flex items-center gap-2.5 px-2 py-1.5">
            <span className="grid size-8 place-items-center rounded-lg bg-[#111] shadow-[0_2px_14px_rgba(52,211,153,.4)]"><TrailMark size={17} className="text-accent" /></span>
            <span className="text-[15px] font-semibold tracking-tight">Backtrail</span>
          </Link>
          {NAV.map(({ to, label, icon: Icon, end }) => (
            <NavLink key={to} to={to} end={end} className={({ isActive }) => `row-hover flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13.5px] ${isActive ? 'bg-white/[.07] font-medium text-ink shadow-[inset_0_1px_0_rgba(255,255,255,.05)]' : 'text-soft'}`}>
              <Icon size={16} />{label}{to === '/graph' && toReview > 0 && <span role="button" onClick={(e) => { e.preventDefault(); nav('/graph?tab=topics') }} className="ml-auto rounded-full bg-[#fbbf2426] px-1.5 text-[11px] tabular-nums text-[#fbbf24]" title="Topics waiting for a decision">{toReview}</span>}
            </NavLink>
          ))}
          <div className="mt-5 px-2.5 pb-1 text-[12px] text-mut" title="Threads whose state is In development.">Active threads</div>
          {live.map((t) => (
            <Link key={t.name} to={`/thread/${encodeURIComponent(t.name)}`} className={`row-hover flex items-center gap-2.5 rounded-lg py-1.5 pr-2.5 text-[13px] text-soft ${nested(t, live) ? 'pl-6' : 'pl-2.5'}`}>
              <span className="size-2 rounded-full" style={{ background: STATE.moving.color }} />
              <span className="truncate">{t.name}</span>
            </Link>
          ))}
          {others.length > 0 && (
            <>
              <button type="button" onClick={toggleOthers} className="mt-4 flex items-center gap-1.5 px-2.5 pb-1 text-left text-[12px] text-mut hover:text-ink" title="Built, parked, dropped and open-idea threads">
                <ChevronRight size={12} className={`transition ${showOthers ? 'rotate-90' : ''}`} />Other threads <span className="tabular-nums">({others.length})</span>
              </button>
              {showOthers && <div>{['built', 'parked', 'open', 'dropped'].map((st) => {
                const g = others.filter((t) => t.state === st)
                return g.length === 0 ? null : (
                  <div key={st}>
                    <div className="px-2.5 pb-0.5 pt-2.5 text-[11px] uppercase tracking-wide text-mut">{STATE[st]?.label ?? st} <span className="tabular-nums">{g.length}</span></div>
                    {g.map((t) => (
                      <Link key={t.name} to={`/thread/${encodeURIComponent(t.name)}`} className={`row-hover flex items-center gap-2.5 rounded-lg py-1.5 pr-2.5 text-[13px] text-soft ${nested(t, g) ? 'pl-6' : 'pl-2.5'}`}>
                        <span className="size-2 shrink-0 rounded-full" style={{ background: STATE[st]?.color ?? '#52525b' }} />
                        <span className="truncate">{t.name}</span>
                      </Link>
                    ))}
                  </div>
                )
              })}</div>}
            </>
          )}
          <div className="mt-auto flex items-center gap-1 px-2.5 pt-4 text-[11.5px] text-mut">
            <span className="min-w-0 flex-1 truncate">{today ? `${today.totals.sessions} sessions, ${today.totals.notes} notes` : ''}</span>
            <button type="button" className="grid size-7 place-items-center rounded-md hover:bg-white/10 hover:text-ink" title="Items: everything filed" onClick={() => setSheet({ kind: 'items' })}><ListChecks size={14} /></button>
            <button type="button" className="grid size-7 place-items-center rounded-md hover:bg-white/10 hover:text-ink" title="Log: every change" onClick={() => setSheet({ kind: 'log' })}><ScrollText size={14} /></button>
          </div>
        </aside>

        <main className="m-2 ml-0 flex min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border border-line bg-bg max-md:m-0 max-md:rounded-none">
          <nav className="flex shrink-0 gap-1 border-b border-line px-3 py-2 md:hidden">
            {NAV.map(({ to, label, icon: Icon, end }) => (
              <NavLink key={to} to={to} end={end} className={({ isActive }) => `flex flex-1 items-center justify-center gap-1.5 rounded-lg py-1.5 text-[13px] ${isActive ? 'bg-white/[.07] font-medium text-ink' : 'text-soft'}`}>
                <Icon size={15} />{label}
              </NavLink>
            ))}
          </nav>
          <header className="flex h-14 shrink-0 items-center gap-3 border-b border-line px-5">
            <h1 className="text-[15px] font-semibold">{title}</h1>
            <div className="relative ml-4 max-w-sm flex-1">
              <SearchIcon size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-mut" />
              <input key={scope} className="input !pl-9" placeholder={scope ? `Search ${scope}` : 'Search everything'} onKeyDown={(e) => { if (e.key === 'Enter') { const el = e.target as HTMLInputElement; const q = el.value.trim(); if (q) { setSheet({ kind: 'search', q, thread: scope }); el.value = '' } } }} />
            </div>
            {!loc.pathname.startsWith('/thread') && <button className="btn primary ml-auto" onClick={() => setCap({ open: true })}><Plus size={15} />Add</button>}
          </header>
          <div className="min-h-0 flex-1 overflow-auto">{children}</div>
        </main>
      </div>
      {sheet && <Modal onClose={() => setSheet(null)}>{sheet.kind === 'search' ? <Search initial={sheet.q} scope={sheet.thread} /> : sheet.kind === 'items' ? <Items /> : <Log />}</Modal>}
      {cap.open && <Capture thread={cap.thread} onClose={() => setCap({ open: false })} onDone={reload} />}
    </ShellCtx.Provider>
  )
}
