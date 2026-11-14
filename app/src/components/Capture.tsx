import { useState } from 'react'
import { Bug, CircleHelp, ListTodo, Scale, X } from 'lucide-react'
import { post } from '@/lib/api'
import '@/next.css'
import { useShell } from '@/components/Shell'

const KINDS = [
  { id: 'step|todo', label: 'Todo', icon: ListTodo },
  { id: 'step|bug', label: 'Bug', icon: Bug },
  { id: 'question|', label: 'Ask Claude', icon: CircleHelp },
  { id: 'decision|', label: 'Decision', icon: Scale },
]

export default function Capture({ thread, onClose, onDone }: { thread?: string; onClose: () => void; onDone: () => void }) {
  const { today } = useShell()
  const threads: any[] = (today?.threads ?? []).filter((t: any) => t.name !== 'Unsorted')
  const [th, setTh] = useState(thread ?? threads.find((t) => t.state === 'moving')?.name ?? threads[0]?.name ?? '')
  const [kind, setKind] = useState('step|todo')
  const [text, setText] = useState('')
  const [body, setBody] = useState('')
  const [err, setErr] = useState('')

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    const [type, tag] = kind.split('|')
    const r = await post('/api/add', { thread: th, type, tag, text, body })
    if (r.ok) { onDone(); window.dispatchEvent(new Event('tm:changed')); onClose() } else setErr(r.error)
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/60 backdrop-blur-sm" onMouseDown={onClose}>
      <form onSubmit={submit} onMouseDown={(e) => e.stopPropagation()} className="nx-now w-[540px] max-w-[94vw] !p-6">
        <div className="relative mb-4 flex items-center"><div className="text-[18px] font-semibold tracking-tight">Add to a thread</div><button type="button" className="btn ghost sm ml-auto" onClick={onClose}><X size={14} /></button></div>
        <label className="relative mb-1 block text-[12px] text-mut">Thread</label>
        <select className="input relative mb-3 !rounded-xl" value={th} onChange={(e) => setTh(e.target.value)}>
          {threads.map((t) => <option key={t.name} value={t.name}>{t.name}</option>)}
        </select>
        <div className="relative mb-3 grid grid-cols-4 gap-1 rounded-full border border-line2 bg-white/[.03] p-1">
          {KINDS.map(({ id, label, icon: Icon }) => (
            <button type="button" key={id} onClick={() => setKind(id)} className={`flex h-8 items-center justify-center gap-1.5 rounded-full text-[13px] transition ${kind === id ? 'bg-white/10 font-medium text-ink' : 'text-mut hover:text-ink'}`}><Icon size={14} />{label}</button>
          ))}
        </div>
        <input className="input relative mb-2 !h-11 !rounded-xl" autoFocus maxLength={110} placeholder={kind.endsWith('bug') ? 'What is broken?' : kind === 'question|' ? 'Your question for Claude' : 'One line'} value={text} onChange={(e) => setText(e.target.value)} />
        <textarea className="input relative !h-24 !rounded-xl py-2.5" maxLength={320} placeholder="Details (optional)" value={body} onChange={(e) => setBody(e.target.value)} />
        <p className="relative mt-2 text-[12px] text-mut">{({ 'step|todo': 'A todo you own. Claude marks it built when it is, and you confirm.', 'step|bug': 'Something broken. Same flow as a todo.', 'question|': 'Claude answers this in its next session. It does not wait on you.', 'decision|': 'A choice you made, kept as standing.' } as any)[kind]}</p>
        {err && <div className="mt-2 text-[12px] text-dropped">{err}</div>}
        <div className="relative mt-4 flex justify-end gap-2"><button type="button" className="btn" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!text.trim()}>Add</button></div>
      </form>
    </div>
  )
}
