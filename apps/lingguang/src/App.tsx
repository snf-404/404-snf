import { Activity, Bluetooth, CirclePause, HeartPulse, Radio, RotateCcw, Wind } from 'lucide-react'
import { useState } from 'react'

import './App.css'
import { PointCloudView } from '@/components/PointCloudView'
import { SignalWave } from '@/components/SignalWave'
import { useRadarSimulation } from '@/hooks/useRadarSimulation'

function App() {
  const [paused, setPaused] = useState(false)
  const [notice, setNotice] = useState('')
  const [viewResetKey, setViewResetKey] = useState(0)
  const telemetry = useRadarSimulation(paused)

  const requestConnection = () => {
    setNotice('BLE v1 尚未写入硬件，当前继续使用安全的演示数据。')
    window.setTimeout(() => {
      setNotice('')
    }, 3600)
  }

  return (
    <div id="container" className="min-h-dvh overflow-x-hidden bg-[#070a0b] text-[#f1f6f4]">
      <div className="ambient-grid" aria-hidden="true" />
      <main className="relative mx-auto flex min-h-dvh w-full max-w-[1480px] flex-col px-4 pb-8 pt-4 sm:px-6 lg:px-8">
        <header className="flex items-center justify-between gap-4 border-b border-white/10 pb-4">
          <div className="flex items-center gap-3">
            <div className="grid size-10 place-items-center border border-[#b7ff35]/50 bg-[#b7ff35]/10 text-[#b7ff35]">
              <Radio size={20} strokeWidth={1.8} />
            </div>
            <div>
              <p className="font-mono text-[10px] uppercase tracking-[0.32em] text-white/45">
                404 SNF · Radar Lab
              </p>
              <h1 className="mt-1 text-base font-semibold tracking-tight sm:text-lg">
                非接触生命体征监测
              </h1>
            </div>
          </div>
          <button
            type="button"
            data-testid="connect-device"
            onClick={requestConnection}
            className="group flex min-h-10 items-center gap-2 border border-white/15 bg-white/[0.04] px-3 font-mono text-xs text-white/75 transition hover:border-[#b7ff35]/50 hover:text-[#b7ff35]"
          >
            <Bluetooth size={15} />
            <span className="hidden sm:inline">连接设备</span>
            <span className="size-1.5 rounded-full bg-amber-400 shadow-[0_0_10px_#fbbf24]" />
          </button>
        </header>

        <section className="mt-4 grid flex-1 gap-4 lg:grid-cols-[minmax(0,1.65fr)_minmax(320px,.75fr)]">
          <div className="relative min-h-[48vh] overflow-hidden border border-white/10 bg-[#0a0e0f]/90 lg:min-h-[680px]">
            <div className="absolute inset-x-0 top-0 z-10 flex items-start justify-between gap-4 p-4 sm:p-5">
              <div>
                <div className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.24em] text-[#b7ff35]">
                  <span className="size-1.5 animate-pulse rounded-full bg-[#b7ff35] shadow-[0_0_12px_#b7ff35]" />
                  Demo stream / 05 Hz
                </div>
                <p className="mt-2 max-w-xs text-xs leading-relaxed text-white/45">
                  合成毫米波点云 · 胸廓位移已放大显示
                </p>
              </div>
              <div className="border border-white/10 bg-black/30 px-3 py-2 text-right backdrop-blur">
                <p className="font-mono text-[9px] uppercase tracking-[0.18em] text-white/35">
                  Target
                </p>
                <p className="mt-1 font-mono text-xs text-white/80">ID 0001 · 1.82 m</p>
              </div>
            </div>

            <PointCloudView
              points={telemetry.points}
              breathingPhase={telemetry.breathingPhase}
              paused={paused}
              viewResetKey={viewResetKey}
            />

            <div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex items-end justify-between gap-3 bg-gradient-to-t from-[#070a0b] via-[#070a0b]/75 to-transparent p-4 pt-16 sm:p-5 sm:pt-20">
              <div className="flex gap-5 font-mono text-[9px] uppercase tracking-[0.16em] text-white/35">
                <span>X ±1.4 m</span>
                <span>Y 0–4 m</span>
                <span>Z ±1.8 m</span>
              </div>
              <button
                type="button"
                data-testid="reset-view"
                onClick={() => {
                  setViewResetKey((value) => value + 1)
                  setNotice('视角已自动归位')
                }}
                className="pointer-events-auto grid size-10 place-items-center border border-white/15 bg-black/40 text-white/55 transition hover:border-white/35 hover:text-white"
                aria-label="重置 3D 视角"
              >
                <RotateCcw size={15} />
              </button>
            </div>
          </div>

          <aside className="flex min-w-0 flex-col gap-4">
            <div className="grid grid-cols-2 gap-3">
              <Metric
                icon={<HeartPulse size={18} />}
                label="心率"
                value={telemetry.heartRate.toFixed(0)}
                unit="BPM"
                tone="lime"
                footnote="置信度 94%"
              />
              <Metric
                icon={<Wind size={18} />}
                label="呼吸"
                value={telemetry.respirationRate.toFixed(1)}
                unit="RPM"
                tone="cyan"
                footnote="置信度 91%"
              />
            </div>

            <div className="border border-white/10 bg-white/[0.025] p-4 sm:p-5">
              <div className="flex items-center justify-between">
                <div>
                  <p className="font-mono text-[9px] uppercase tracking-[0.24em] text-white/35">
                    Vital waveforms
                  </p>
                  <h2 className="mt-1 text-sm font-medium">实时生命波形</h2>
                </div>
                <Activity size={17} className="text-[#b7ff35]" />
              </div>
              <div className="mt-5 space-y-5">
                <SignalWave values={telemetry.heartWave} color="#b7ff35" label="ECG proxy" />
                <SignalWave values={telemetry.breathWave} color="#45d9ff" label="Respiration" />
              </div>
            </div>

            <div className="grid grid-cols-3 border border-white/10 bg-white/[0.025]">
              <StatusCell label="信号质量" value="优秀" accent />
              <StatusCell label="运动干扰" value="低" />
              <StatusCell label="雷达温度" value="42°C" />
            </div>

            <div className="mt-auto border border-amber-300/20 bg-amber-300/[0.05] p-4">
              <div className="flex items-start gap-3">
                <span className="mt-1 size-2 shrink-0 rounded-full bg-amber-300 shadow-[0_0_12px_#fcd34d]" />
                <div>
                  <p className="text-xs font-medium text-amber-100">演示数据</p>
                  <p className="mt-1 text-[11px] leading-relaxed text-amber-100/50">
                    硬件端 BLE v1 尚未实现。当前点云、姿态轮廓和生命体征均为模拟，不用于医疗判断。
                  </p>
                </div>
              </div>
            </div>

            <button
              type="button"
              data-testid="toggle-stream"
              onClick={() => {
                setPaused((value) => !value)
              }}
              className="flex min-h-12 w-full items-center justify-center gap-2 bg-[#b7ff35] px-4 font-mono text-xs font-semibold uppercase tracking-[0.14em] text-[#111704] transition hover:bg-[#ceff78]"
            >
              <CirclePause size={16} />
              {paused ? '继续演示数据流' : '暂停数据流'}
            </button>
          </aside>
        </section>
      </main>

      {notice !== '' && (
        <div className="toast" role="status">
          <span className="size-1.5 rounded-full bg-[#b7ff35]" />
          {notice}
        </div>
      )}
    </div>
  )
}

type MetricProps = {
  icon: React.ReactNode
  label: string
  value: string
  unit: string
  tone: 'lime' | 'cyan'
  footnote: string
}

function Metric({ icon, label, value, unit, tone, footnote }: MetricProps) {
  const color = tone === 'lime' ? 'text-[#b7ff35]' : 'text-[#45d9ff]'
  return (
    <div className="border border-white/10 bg-white/[0.025] p-4 sm:p-5">
      <div className={`flex items-center gap-2 ${color}`}>
        <span>{icon}</span>
        <span className="font-mono text-[9px] uppercase tracking-[0.2em]">{label}</span>
      </div>
      <div className="mt-5 flex items-end gap-1.5">
        <span className="font-mono text-4xl font-light tracking-[-0.08em] sm:text-5xl">
          {value}
        </span>
        <span className="mb-1 font-mono text-[9px] text-white/35">{unit}</span>
      </div>
      <p className="mt-3 font-mono text-[9px] text-white/35">{footnote}</p>
    </div>
  )
}

function StatusCell({
  label,
  value,
  accent = false,
}: {
  label: string
  value: string
  accent?: boolean
}) {
  return (
    <div className="border-r border-white/10 p-3 last:border-r-0 sm:p-4">
      <p className="text-[9px] text-white/35">{label}</p>
      <p className={`mt-2 font-mono text-xs ${accent ? 'text-[#b7ff35]' : 'text-white/75'}`}>
        {value}
      </p>
    </div>
  )
}

export default App
