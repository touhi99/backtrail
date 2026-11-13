import { useState, type ReactNode } from 'react'
import { ChevronRight } from 'lucide-react'

/** A closed-by-default section: one line with a count, opened on click. */
export default function Fold({ label, count, children }: { label: string; count: number; children: ReactNode }) {
  const [open, setOpen] = useState(false)
  if (count === 0) return null
  return (
    <div className="first:border-t-0 border-t border-line">
      <button type="button" onClick={() => setOpen((v) => !v)} className="flex w-full items-center gap-1.5 px-5 py-3 text-left text-[13px] text-mut hover:text-ink" aria-expanded={open}>
        <ChevronRight size={13} className={`transition ${open ? 'rotate-90' : ''}`} />{label} <span className="tabular-nums">({count})</span>
      </button>
      {open && children}
    </div>
  )
}
