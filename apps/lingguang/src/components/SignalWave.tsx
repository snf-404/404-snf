type SignalWaveProps = {
  values: number[]
  color: string
  label: string
}

export function SignalWave({ values, color, label }: SignalWaveProps) {
  const width = 320
  const height = 54
  const safeValues = values.length > 1 ? values : [0, 0]
  const path = safeValues
    .map((value, index) => {
      const x = (index / (safeValues.length - 1)) * width
      const y = height / 2 - value * (height * 0.34)
      return `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`
    })
    .join(' ')

  return (
    <div>
      <div className="mb-2 flex items-center justify-between font-mono text-[9px] uppercase tracking-[0.14em] text-white/30">
        <span>{label}</span>
        <span>Live</span>
      </div>
      <svg
        viewBox={`0 0 ${String(width)} ${String(height)}`}
        className="h-14 w-full overflow-visible"
        role="img"
        aria-label={`${label} 模拟波形`}
      >
        <path
          d={`M 0 ${String(height / 2)} H ${String(width)}`}
          stroke="rgba(255,255,255,.08)"
          strokeWidth="1"
        />
        <path
          d={path}
          fill="none"
          stroke={color}
          strokeWidth="1.8"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
    </div>
  )
}
