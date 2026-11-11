import { post } from '@/lib/api'
import { useShell } from '@/components/Shell'

/** What an item can do next: suggested items are accepted or rejected, a kept decision can be marked replaced. */
/** A small "Filed by You / Claude" tag. */
export const Who = ({ i }: { i: any }) => <span className="nx-chip !h-[18px] !px-2 !text-[11px]" style={{ background: i.origin === 'owner' ? '#f4f4f51a' : '#60a5fa1f', color: i.origin === 'owner' ? '#f4f4f5' : '#60a5fa' }} title={i.origin === 'owner' ? 'You typed this in the app' : 'Claude filed this from a session'}>{i.origin === 'owner' ? 'You' : 'Claude'}</span>

/** Re-file an item you typed under the wrong project. */
function Move({ i, done }: { i: any; done: () => void }) {
  const { today } = useShell()
  if (i.origin !== 'owner' || i.state === 'done') return null
  return (
    <select className="input !h-7 !w-auto !rounded-lg !px-2 !text-[12px] text-mut" value="" title="Typed in the wrong project? Move it." onChange={async (e) => { if (e.target.value) { await post('/api/move', { id: i.id, thread: e.target.value }); done() } }}>
      <option value="">Move to…</option>
      {(today?.threads ?? []).filter((t: any) => t.name !== i.thread && t.name !== 'Unsorted').map((t: any) => <option key={t.name}>{t.name}</option>)}
    </select>
  )
}

export default function ItemActions({ i, done }: { i: any; done: () => void }) {
  const mv = <Move i={i} done={done} />
  const run = async (url: string, body: any) => { await post(url, body); done() }
  if (i.state === 'proposed')
    return (
      <div className="flex gap-2">
        <button className="btn sm primary" onClick={() => run('/api/review', { id: i.id, accept: true })}>Accept</button>
        <button className="btn sm" title="Was true once, no longer applies. Kept on file, out of the way." onClick={() => { const reason = prompt('Why is it not valid now?'); if (reason !== null) run('/api/review', { id: i.id, accept: false, park: true, reason }) }}>Not valid now</button>
        <button className="btn sm" title="Wrong or not wanted." onClick={() => { const reason = prompt('Why reject? (kept with the item)'); if (reason !== null) run('/api/review', { id: i.id, accept: false, reason }) }}>Reject</button>
        {mv}
      </div>
    )
  if (i.type === 'decision' && i.state === 'settled' && !(i.stale ?? '').startsWith('superseded'))
    return <button className="btn sm" onClick={() => { const reason = prompt('What replaced it?'); if (reason !== null) run('/api/supersede', { id: i.id, reason }) }}>Mark replaced</button>
  return mv
}
