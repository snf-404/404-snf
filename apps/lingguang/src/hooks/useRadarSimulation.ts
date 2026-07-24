import { useEffect, useMemo, useState } from 'react'

const POINT_COUNT = 260

function pseudoRandom(index: number, salt: number): number {
  const value = Math.sin(index * 91.77 + salt * 31.19) * 43758.5453
  return value - Math.floor(value)
}

function createPointCloud(phase: number): Float32Array {
  const points = new Float32Array(POINT_COUNT * 3)
  for (let index = 0; index < POINT_COUNT; index += 1) {
    const angle = pseudoRandom(index, 1) * Math.PI * 2
    const bodyBand = pseudoRandom(index, 2)
    const z = bodyBand * 2.25 - 1.05
    const isTorso = z > -0.65 && z < 0.55
    const radius = isTorso
      ? 0.48 + pseudoRandom(index, 3) * 0.25
      : 0.18 + pseudoRandom(index, 3) * 0.17
    const breathing = isTorso ? Math.sin(phase) * 0.065 : 0
    points[index * 3] =
      Math.cos(angle) * (radius + breathing) + (pseudoRandom(index, 4) - 0.5) * 0.12
    points[index * 3 + 1] =
      Math.sin(angle) * (radius * 0.52 + breathing) + (pseudoRandom(index, 5) - 0.5) * 0.09
    points[index * 3 + 2] = z
  }
  return points
}

function createWave(length: number, phase: number, kind: 'heart' | 'breath'): number[] {
  return Array.from({ length }, (_, index) => {
    const x = phase + index * (kind === 'heart' ? 0.42 : 0.13)
    if (kind === 'breath') return Math.sin(x) * 0.82 + Math.sin(x * 2) * 0.08
    const beat = ((x % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2)
    const spike = Math.exp(-Math.pow((beat - 4.25) * 5.2, 2)) * 1.9
    const dip = Math.exp(-Math.pow((beat - 4.05) * 7, 2)) * -0.55
    return Math.sin(x) * 0.12 + spike + dip - 0.15
  })
}

export function useRadarSimulation(paused: boolean) {
  const [phase, setPhase] = useState(0)

  useEffect(() => {
    if (paused) return undefined
    const timer = window.setInterval(() => {
      setPhase((value) => value + 0.12)
    }, 100)
    return () => {
      window.clearInterval(timer)
    }
  }, [paused])

  return useMemo(
    () => ({
      heartRate: 72 + Math.sin(phase * 0.27) * 2.2,
      respirationRate: 15.8 + Math.sin(phase * 0.19) * 0.7,
      breathingPhase: phase,
      points: createPointCloud(phase),
      heartWave: createWave(58, phase, 'heart'),
      breathWave: createWave(58, phase, 'breath'),
    }),
    [phase],
  )
}
