import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { Check, ExternalLink, Search as SearchIcon, X } from 'lucide-react'
import { post } from '@/lib/api'
import type { Done, EntityAdded, KgSuggest } from '@/lib/types'

export const KIND_C: Record<string, string> = { project: '#60a5fa', service: '#c084fc', tool: '#fbbf24', model: '#f87171', site: '#34d399', topic: '#f472b6' }
export const KINDS: [string, string][] = [
  ['topic', 'a subject or idea that comes up'],
  ['project', 'something being built'],
  ['tool', 'software that gets used'],
  ['service', 'something that runs in the background, own or hosted'],
  ['model', 'a named AI model, like Haiku or Qwen'],
  ['site', 'a website or company, like Google or Rivalsmeta'],
]

function Marked({ text, term }: { text: string; term: string }) {
  const parts = text.split(new RegExp(`(${term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'i'))
  return <>{parts.map((p, i) => (p.toLowerCase() === term.toLowerCase() ? <b key={i} className="font-semibold text-ink">{p}</b> : <span key={i}>{p}</span>))}</>
}

function Bar({ n, max, color = '#8b8b95' }: { n: number; max: number; color?: string }) {
  return <span className="block h-1.5 w-full overflow-hidden rounded-full bg-white/[.06]"><span className="block h-full rounded-full" style={{ width: `${Math.max(6, (n / max) * 100)}%`, background: color }} /></span>
}

type Added = { entity: string; linked: number; unread: number; usdEach: number; started?: number }

export default function Topics({ sug, onChanged }: { sug: KgSuggest | null; onChanged: () => void }) {
  const terms = sug?.entities ?? []
  const aliases = sug?.aliases ?? []
  const [pick, setPick] = useState('')
  const [kind, setKind] = useState('topic')
  const [more, setMore] = useState(false)
  const [added, setAdded] = useState<Added | null>(null)
  const [hid, setHid] = useState('')
  const hidden = sug?.hidden ?? []
  const top = useRef<HTMLDivElement>(null)
  const cur = terms.find((t) => t.term === pick) ?? terms[0]
  const max = Math.max(1, ...terms.map((t) => t.turns))
  useEffect(() => { setMore(false); setKind('topic') }, [cur?.term])

  const send = async (name: string, k: string, aliasOf = '') => {
    const r = await post<EntityAdded>('/api/kg/entity', { name, kind: k, alias_of: aliasOf })
    if (r?.entity) { setAdded({ entity: r.entity, linked: r.linked ?? 0, unread: r.unread ?? 0, usdEach: r.usd_each ?? 0.02 }); setHid('') }
    if (k === 'ignore') { setHid(name); setAdded(null) }
    setPick('')
    onChanged()
    top.current?.scrollIntoView({ block: 'start' })
  }
  const restore = async (term: string) => {
    await post('/api/kg/entity', { name: term, kind: 'unignore' })
    setHid('')
    setPick(term)
    onChanged()
    top.current?.scrollIntoView({ block: 'start' })
  }
  const read = async () => {
    if (!added) return
    const r = await post<Done & { started?: number }>('/api/kg/extract', { entity: added.entity, limit: 30 })
    if (r?.ok) setAdded({ ...added, started: r.started })
  }

  const examples = cur?.examples ?? []
  const shown = more ? examples : examples.slice(0, 2)
  const rest = terms.filter((t) => t.term !== cur?.term)

  return (
    <div ref={top} className="flex scroll-mt-3 flex-col gap-3">
      {added && (
        <div className="rounded-[14px] border border-[#34d39944] bg-[#34d3991a] p-3">
          <div className="flex items-start gap-2">
            <span className="mt-0.5 grid size-5 shrink-0 place-items-center rounded-full bg-accent text-[#04130c]"><Check size={13} strokeWidth={3} /></span>
            <div className="min-w-0 flex-1 text-[13px] leading-snug">
              <b>{added.entity}</b> is in the graph with {added.linked} {added.linked === 1 ? 'claim' : 'claims'}.
              {added.started ? <span className="text-soft"> Reading {added.started} sessions in the background; reload in a minute.</span>
                : added.unread > 0 ? <span className="text-soft"> {added.unread} sessions mention it and are not read yet.</span> : <span className="text-soft"> No unread sessions mention it.</span>}
            </div>
            <button onClick={() => setAdded(null)} title="Dismiss" className="text-mut hover:text-ink"><X size={14} /></button>
          </div>
          {added.unread > 0 && !added.started && (
            <button className="btn primary sm mt-2.5 w-full justify-center" onClick={read}>Read {Math.min(added.unread, 30)} sessions now · about ${(Math.min(added.unread, 30) * added.usdEach).toFixed(2)}</button>
          )}
        </div>
      )}

      {hid && (
        <div className="flex items-center gap-2 rounded-[14px] border border-line2 bg-white/[.04] px-3 py-2 text-[13px]">
          <span className="min-w-0 flex-1 truncate"><b>{hid}</b> hidden.</span>
          <button className="btn sm" onClick={() => restore(hid)}>Undo</button>
          <button onClick={() => setHid('')} title="Dismiss" className="text-mut hover:text-ink"><X size={14} /></button>
        </div>
      )}

      {aliases.length > 0 && (
        <div className="rounded-[14px] border border-line bg-white/[.025] p-3">
          <div className="mb-1.5 text-[12.5px] font-semibold">Same thing, spelled differently?</div>
          <div className="flex flex-col gap-1.5">
            {aliases.map((a) => (
              <div key={a.alias} className="flex items-center gap-2 text-[13px]">
                <span className="min-w-0 flex-1 truncate"><b>{a.alias}</b> <span className="text-mut">= {a.entity}</span></span>
                <button className="btn sm" onClick={() => send(a.alias, 'project', a.entity)}>Merge</button>
                <button className="btn ghost sm" title="Not the same: hide this suggestion" onClick={() => send(a.alias, 'ignore')}>Keep apart</button>
              </div>
            ))}
          </div>
        </div>
      )}

      {cur ? (
        <div className="rounded-[18px] border border-line2 bg-white/[.035] p-4 shadow-[0_8px_30px_rgba(0,0,0,.25)]">
          <div className="text-[12px] text-mut">Should this be in the graph?</div>
          <div className="mt-0.5 flex items-baseline gap-2">
            <div className="truncate text-[26px] font-semibold leading-tight tracking-tight">{cur.term}</div>
            <Link to={`/search?q=${encodeURIComponent(cur.term)}`} className="inline-flex items-center gap-1 text-[12px] text-mut hover:text-ink" title="Search every mention"><SearchIcon size={12} />all mentions</Link>
          </div>
          <div className="mt-2.5 flex items-center gap-3">
            <div className="w-32 shrink-0"><Bar n={cur.turns} max={max} color="#34d399" /></div>
            <div className="text-[12.5px] text-soft"><b className="tabular-nums text-ink">{cur.turns}</b> mentions in <b className="tabular-nums text-ink">{cur.sessions}</b> sessions</div>
          </div>

          <div className="mt-3.5 flex flex-col gap-2">
            {shown.map((x, i) => (
              <Link key={i} to={`/source/${x.source_id}`} className="group block rounded-[10px] border-l-2 border-line2 bg-white/[.02] px-3 py-2 text-[12.5px] leading-snug text-soft hover:border-accent hover:bg-white/[.05]">
                <span className="mb-0.5 flex items-center gap-1 text-[11px] text-mut">{x.date}<ExternalLink size={10} className="opacity-0 group-hover:opacity-100" /></span>
                “<Marked text={x.text} term={cur.term} />”
              </Link>
            ))}
            {examples.length > 2 && <button className="self-start text-[12px] text-mut underline decoration-line2 hover:text-ink" onClick={() => setMore(!more)}>{more ? 'Show fewer quotes' : `Show ${examples.length - 2} more quotes`}</button>}
          </div>

          <div className="mt-4 text-[12px] text-mut">Add it as</div>
          <div className="mt-1.5 grid grid-cols-2 gap-1.5">
            {KINDS.map(([k, d], i) => (
              <button key={k} type="button" onClick={() => setKind(k)} className={`rounded-[10px] border px-2.5 py-1.5 text-left transition-colors ${i === KINDS.length - 1 && KINDS.length % 2 ? 'col-span-2' : ''}`}
                style={{ borderColor: kind === k ? KIND_C[k] : 'var(--color-line2)', background: kind === k ? `${KIND_C[k]}1f` : 'rgba(255,255,255,.02)' }}>
                <span className="flex items-center gap-1.5 text-[13px] font-medium"><span className="size-2 rounded-full" style={{ background: KIND_C[k] }} />{k}</span>
                <span className="mt-0.5 block text-[11px] leading-tight text-mut">{d}</span>
              </button>
            ))}
          </div>
          <div className="mt-3 flex items-center gap-2">
            <button className="nx-btn flex-1" style={{ height: 38 }} onClick={() => send(cur.term, kind)}>Add “{cur.term}” to the graph</button>
            <button className="btn" style={{ height: 38 }} title="Hide it for good" onClick={() => send(cur.term, 'ignore')}>Not a topic</button>
          </div>
        </div>
      ) : (
        <div className="rounded-[14px] border border-line bg-white/[.025] p-4 text-[13px] text-soft">Nothing to review. New names show up here when they come up often.</div>
      )}

      {rest.length > 0 && (
        <div>
          <div className="mb-1.5 px-1 text-[12px] text-mut">Up next</div>
          <div className="flex flex-col">
            {rest.map((t) => (
              <button key={t.term} type="button" onClick={() => setPick(t.term)} className="row-hover grid grid-cols-[1fr_84px_34px] items-center gap-3 rounded-[10px] px-2 py-1.5 text-left text-[13px]">
                <span className="truncate">{t.term}</span><Bar n={t.turns} max={max} /><span className="text-right text-[12px] tabular-nums text-mut">{t.turns}</span>
              </button>
            ))}
          </div>
        </div>
      )}
      {hidden.length > 0 && (
        <details className="rounded-[14px] border border-line bg-white/[.02] px-3 py-2">
          <summary className="cursor-pointer text-[12.5px] text-mut">Hidden ({hidden.length})</summary>
          <div className="mt-1.5 flex flex-col gap-1">
            {hidden.map((h) => (
              <div key={h} className="flex items-center gap-2 text-[13px]"><span className="min-w-0 flex-1 truncate">{h}</span><button className="btn ghost sm" onClick={() => restore(h)}>Restore</button></div>
            ))}
          </div>
        </details>
      )}
    </div>
  )
}
