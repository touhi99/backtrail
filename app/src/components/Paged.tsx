import { useEffect, useState, type ReactNode } from 'react'
import { ChevronLeft, ChevronRight } from 'lucide-react'

export default function Paged<T>({ items, size, children, resetKey = '' }: { items: T[]; size: number; resetKey?: string; children: (rows: T[]) => ReactNode }) {
  const [page, setPage] = useState(0)
  const pages = Math.max(1, Math.ceil(items.length / size))
  useEffect(() => { setPage((p) => Math.min(p, pages - 1)) }, [pages])
  useEffect(() => { setPage(0) }, [resetKey])
  const at = Math.min(page, pages - 1), from = at * size
  return (
    <>
      {children(items.slice(from, from + size))}
      {items.length > size && (
        <div className="flex items-center justify-between px-5 py-2.5 text-[12.5px] text-mut">
          <span className="tabular-nums">{from + 1}–{Math.min(from + size, items.length)} of {items.length}</span>
          <div className="flex gap-1.5">
            <button type="button" aria-label="Previous page" disabled={at === 0} onClick={() => setPage(at - 1)} className="btn sm !px-2 disabled:opacity-30"><ChevronLeft size={14} /></button>
            <button type="button" aria-label="Next page" disabled={at >= pages - 1} onClick={() => setPage(at + 1)} className="btn sm !px-2 disabled:opacity-30"><ChevronRight size={14} /></button>
          </div>
        </div>
      )}
    </>
  )
}
