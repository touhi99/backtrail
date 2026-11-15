import { Link } from 'react-router-dom'
import { dayFmt } from '@/lib/api'
import { useShell } from '@/components/Shell'
import { STATE_HEX, nextStep, useDetails } from '@/lib/next'
import { Ring } from '@/components/Viz'
import { TrailCard } from '@/pages/Trail'
import '@/next.css'

// Who confirmed a decision, shown next to each one.
const HOW: Record<string, string> = { 'owner-stated': '#34d399', 'owner-accepted': '#60a5fa', 'from a note': '#8b8b95' }

// Home: one card per project in motion, with its next step and the decisions made last. No review queue.
export default function Home() {
  const { today } = useShell()
  const threads: any[] = today?.threads ?? []
  const moving = threads.filter((t) => t.state === 'moving')
  const details = useDetails(moving.map((t) => t.name))
  const asks: any[] = today?.asks ?? []
  const when = new Date().toLocaleDateString('en-GB', { weekday: 'long', day: 'numeric', month: 'long' })

  return (
    <div className="nx-page">
      <div className="pointer-events-none absolute inset-x-0 top-0 h-[420px]" style={{ background: 'radial-gradient(900px 420px at 30% -80px, rgba(52,211,153,.09), transparent 70%)' }} />
      <div className="relative mx-auto flex max-w-[1080px] flex-col gap-7">
        <header className="nx-in">
          <div className="text-[13px] text-mut">{when}</div>
          <h2 className="text-[28px] font-semibold leading-tight tracking-tight">Home</h2>
        </header>

        <TrailCard />

        {asks.length > 0 && (
          <section className="nx-panel nx-in !rounded-2xl px-5 py-4 text-[13.5px] text-soft">
            {asks.length} {asks.length === 1 ? 'question' : 'questions'} typed in the app, waiting for Claude's next session.
          </section>
        )}

        <section className="nx-in grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
          {moving.map((t) => {
            const c = STATE_HEX[t.state], tot = t.done_s + t.open_s, nx = nextStep(details[t.name])
            const decided: any[] = (details[t.name]?.decided ?? []).slice(0, 3)
            return (
              <Link key={t.name} to={`/thread/${encodeURIComponent(t.name)}`} className="nx-panel flex flex-col gap-3 !rounded-2xl p-5 transition hover:-translate-y-px hover:border-[#34343c]">
                <div className="flex items-center gap-3">
                  <span className="nx-glyph size-9 text-[15px] font-semibold" style={{ background: c + '1f', color: c }}>{t.name[0]}</span>
                  <span className="min-w-0 flex-1 truncate text-[16px] font-semibold">{t.name}</span>
                  <Ring value={tot ? t.done_s / tot : 0} color="#34d399" size={46} stroke={5}><span className="text-[10px] font-semibold tabular-nums text-soft">{t.done_s}/{tot}</span></Ring>
                </div>
                <div className="text-[13.5px] text-mut">Next: <span className="text-ink">{nx ?? 'no open step'}</span></div>
                <div className="flex flex-col gap-1.5 border-t border-line pt-3">
                  <div className="text-[11.5px] uppercase tracking-wide text-mut">Decided</div>
                  {decided.length === 0 && <div className="text-[13px] text-mut">No settled decisions yet.</div>}
                  {decided.map((x) => (
                    <div key={x.id} className="flex items-baseline gap-2 text-[13px]">
                      <span className="w-[52px] shrink-0 tabular-nums text-mut">{x.day === 'undated' ? x.day : dayFmt(x.day)}</span>
                      <span className="min-w-0 flex-1 line-clamp-2">{x.title}</span>
                      <span className="size-2 shrink-0 translate-y-[-1px] rounded-full" title={x.how} style={{ background: HOW[x.how] ?? '#8b8b95' }} />
                    </div>
                  ))}
                </div>
              </Link>
            )
          })}
        </section>

        <p className="nx-in text-[12.5px] text-mut">
          Dot colour: green is the owner's own words, blue is accepted in the app, grey is from a note. Search above covers sessions, notes and artifacts.
        </p>
      </div>
    </div>
  )
}
