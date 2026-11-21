import { useApi } from '@/lib/api'
import type { LogEntry } from '@/lib/types'
import Paged from '@/components/Paged'
import '@/next.css'

const ACTOR: Record<string, string> = { owner: 'You', claude: 'Claude' }

export default function Log() {
  const { data } = useApi<LogEntry[]>('/api/log')
  return (
    <div>
      <div className="nx-in"><div className="text-[13px] text-mut">Every change, newest first</div><h2 className="text-[30px] font-semibold leading-tight tracking-tight">Log</h2></div>
      <section className="nx-panel nx-in mt-5 overflow-hidden" style={{ '--d': '60ms' }}>
        <Paged items={data ?? []} size={20}>{(pageRows) => pageRows.map((x, i) => (
          <div key={i} className="nx-row" style={{ gridTemplateColumns: '92px 84px minmax(0,1fr)', cursor: 'default', padding: '11px 18px' }}>
            <span className="text-[12.5px] tabular-nums text-mut">{(x.ts ?? '').replace('T', ' ').slice(5, 16)}</span>
            <span className="chip !h-[22px] justify-self-start"><span className="size-1.5 rounded-full" style={{ background: x.actor === 'owner' ? '#34d399' : '#60a5fa' }} />{ACTOR[x.actor] ?? x.actor}</span>
            <span className="min-w-0 truncate text-[13.5px]">{x.action} <span className="text-soft">{x.detail || x.target}</span></span>
          </div>
        ))}</Paged>
      </section>
    </div>
  )
}
