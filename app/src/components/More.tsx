import { useState, type ReactNode } from 'react'

export default function More({ label = 'Detail', children, className = '' }: { label?: string; children: ReactNode; className?: string }) {
  const [open, setOpen] = useState(false)
  return open
    ? <div className={className}>{children}<button type="button" className="ml-2 text-[12px] text-mut hover:text-ink" onClick={() => setOpen(false)}>Hide</button></div>
    : <button type="button" className={`text-[12px] text-mut hover:text-ink ${className}`} onClick={() => setOpen(true)}>{label}</button>
}
