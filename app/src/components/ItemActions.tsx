import { post } from '@/lib/api'
import { useShell } from '@/components/Shell'
import type { Item } from '@/lib/types'

export const Who = ({ i }: { i: Pick<Item, 'origin'> }) => <span className="nx-chip !h-[18px] !px-2 !text-[11px]" style={{ background: i.origin === 'owner' ? '#f4f4f51a' : '#60a5fa1f', color: i.origin === 'owner' ? '#f4f4f5' : '#60a5fa' }} title={i.origin === 'owner' ? 'You typed this in the app' : 'Claude filed this from a session'}>{i.origin === 'owner' ? 'You' : 'Claude'}</span>

function Move({ i, done }: { i: Item; done: () => void }) {
  const { today } = useShell()
  if (i.origin !== 'owner' || i.state === 'done') return null
  return (
    <select className="input !h-7 !w-auto !rounded-lg !px-2 !text-[12px] text-mut" value="" title="Typed in the wrong thread? Move it." onChange={async (e) => { if (e.target.value) { await post('/api/move', { id: i.id, thread: e.target.value }); done() } }}>
      <option value="">Move to…</option>
      {(today?.threads ?? []).filter((t) => t.name !== i.thread && t.name !== 'Unsorted').map((t) => <option key={t.name}>{t.name}</option>)}
    </select>
  )
}

export default function ItemActions({ i, done }: { i: Item; done: () => void }) {
  const mv = <Move i={i} done={done} />
  const run = async (url: string, body: Record<string, unknown>) => { await post(url, body); done() }
  const held = i.review?.startsWith('held|') ? i.review.slice(5) : ''
  const reject = <button className="btn sm" title="Wrong or not wanted." onClick={() => { const reason = prompt('Why reject? (kept with the item)'); if (reason !== null) run('/api/review', { id: i.id, accept: false, reason }) }}>Reject</button>
  if (i.review === 'auto' && i.state === 'settled')
    return (
      <div className="flex items-center gap-2">
        <span className="text-[12px] text-mut" title="You said this in a session in so many words, so it was kept without review.">Kept automatically</span>
        {reject}
        {i.type === 'decision' && !(i.stale ?? '').startsWith('superseded') && <button className="btn sm" onClick={() => { const reason = prompt('What replaced it?'); if (reason !== null) run('/api/supersede', { id: i.id, reason }) }}>Mark replaced</button>}
      </div>
    )
  if (i.state === 'proposed')
    return (
      <div className="flex items-center gap-2">
        {held && <span className="nx-chip whitespace-nowrap" style={{ background: '#fbbf2418', color: '#fbbf24' }} title={`Not kept automatically: it ${held}`}>May repeat</span>}
        <button className="btn sm primary" onClick={() => run('/api/review', { id: i.id, accept: true })}>Accept</button>
        <button className="btn sm" title="Was true once, no longer applies. Kept on file, out of the way." onClick={() => { const reason = prompt('Why is it not valid now?'); if (reason !== null) run('/api/review', { id: i.id, accept: false, park: true, reason }) }}>Not valid now</button>
        {reject}
        {mv}
      </div>
    )
  if (i.type === 'decision' && i.state === 'settled' && !(i.stale ?? '').startsWith('superseded'))
    return <button className="btn sm" onClick={() => { const reason = prompt('What replaced it?'); if (reason !== null) run('/api/supersede', { id: i.id, reason }) }}>Mark replaced</button>
  return mv
}
