import type { ReactNode } from 'react'

export function Ring({ value, size = 44, stroke = 5, color, children }: { value: number; size?: number; stroke?: number; color: string; children?: ReactNode }) {
  const r = (size - stroke) / 2, c = 2 * Math.PI * r
  return (
    <div className="relative grid shrink-0 place-items-center" style={{ width: size, height: size }}>
      <svg width={size} height={size} style={{ transform: 'rotate(-90deg)' }}>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="#26262b" strokeWidth={stroke} />
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={color} strokeWidth={stroke} strokeLinecap="round" strokeDasharray={`${c * value} ${c}`} />
      </svg>
      <div className="absolute text-center">{children}</div>
    </div>
  )
}

export function SegRing({ segs, size = 84, stroke = 10 }: { segs: { v: number; c: string }[]; size?: number; stroke?: number }) {
  const r = (size - stroke) / 2, c = 2 * Math.PI * r, tot = segs.reduce((a, s) => a + s.v, 0) || 1
  let off = 0
  return (
    <svg width={size} height={size} style={{ transform: 'rotate(-90deg)' }}>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="#1d1d21" strokeWidth={stroke} />
      {segs.map((s, i) => { const len = (s.v / tot) * c; const el = <circle key={i} cx={size / 2} cy={size / 2} r={r} fill="none" stroke={s.c} strokeWidth={stroke} strokeDasharray={`${Math.max(len - 3, 0)} ${c}`} strokeDashoffset={-off} strokeLinecap="round" />; off += len; return el })}
    </svg>
  )
}

export function Spark({ data, color = '#34d399', w = 220, h = 54 }: { data: number[]; color?: string; w?: number; h?: number }) {
  const max = Math.max(1, ...data), step = w / (data.length - 1)
  const pts = data.map((v, i) => [i * step, h - 6 - (v / max) * (h - 14)] as const)
  const line = pts.map(([x, y], i) => `${i ? 'L' : 'M'}${x.toFixed(1)},${y.toFixed(1)}`).join(' ')
  const last = pts[pts.length - 1]
  return (
    <svg viewBox={`0 0 ${w} ${h}`} width="100%" height={h} preserveAspectRatio="none" style={{ overflow: 'visible' }}>
      <defs><linearGradient id="spk" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor={color} stopOpacity=".38" /><stop offset="1" stopColor={color} stopOpacity="0" /></linearGradient></defs>
      <path d={`${line} L${w},${h} L0,${h} Z`} fill="url(#spk)" />
      <path d={line} fill="none" stroke={color} strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke" />
      <circle cx={last[0]} cy={last[1]} r="3.5" fill={color} />
    </svg>
  )
}
