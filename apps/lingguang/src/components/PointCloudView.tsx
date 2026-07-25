import { OrbitControls } from '@react-three/drei'
import { Canvas, useFrame } from '@react-three/fiber'
import { useMemo, useRef } from 'react'
import * as THREE from 'three'

import { useTranslation } from '@/i18n'

type PointCloudViewProps = {
  points: Float32Array
  targetPoints: Float32Array
  breathingPhase: number
  paused: boolean
  viewResetKey: number
  hasSpatialData: boolean
  hasPresence: boolean
}

function RadarPoints({ points, paused }: Pick<PointCloudViewProps, 'points' | 'paused'>) {
  const ref = useRef<THREE.Points>(null)
  const colors = useMemo(() => {
    const buffer = new Float32Array(points.length)
    const color = new THREE.Color()
    for (let index = 0; index < points.length / 3; index += 1) {
      const height = points[index * 3 + 2] ?? 0
      color.set(height > 0.25 ? '#5278e8' : height > -0.55 ? '#8aa9f3' : '#b6c9f2')
      color.toArray(buffer, index * 3)
    }
    return buffer
  }, [points])

  useFrame((_, delta) => {
    if (!paused && ref.current !== null) ref.current.rotation.z += delta * 0.035
  })

  return (
    <points ref={ref}>
      <bufferGeometry>
        <bufferAttribute attach="attributes-position" args={[points, 3]} />
        <bufferAttribute attach="attributes-color" args={[colors, 3]} />
      </bufferGeometry>
      <pointsMaterial
        size={0.045}
        vertexColors
        transparent
        opacity={0.88}
        sizeAttenuation
        depthWrite={false}
        blending={THREE.AdditiveBlending}
      />
    </points>
  )
}

function BodyGuide({ breathingPhase }: Pick<PointCloudViewProps, 'breathingPhase'>) {
  const scale = 1 + Math.sin(breathingPhase) * 0.025
  return (
    <group scale={[scale, 1, scale]}>
      <mesh position={[0, 0, 0.58]}>
        <sphereGeometry args={[0.18, 20, 16]} />
        <meshBasicMaterial color="#5278e8" transparent opacity={0.12} wireframe />
      </mesh>
      <mesh position={[0, 0, -0.05]} scale={[0.58, 0.32, 1]}>
        <capsuleGeometry args={[0.47, 0.9, 8, 16]} />
        <meshBasicMaterial color="#7f9eeb" transparent opacity={0.13} wireframe />
      </mesh>
    </group>
  )
}

function TargetMarkers({ points }: { points: Float32Array }) {
  if (points.length === 0) return null
  return (
    <points>
      <bufferGeometry>
        <bufferAttribute attach="attributes-position" args={[points, 3]} />
      </bufferGeometry>
      <pointsMaterial size={0.16} color="#315fd1" sizeAttenuation transparent opacity={0.95} />
    </points>
  )
}

export function PointCloudView({
  points,
  targetPoints,
  breathingPhase,
  paused,
  viewResetKey,
  hasSpatialData,
  hasPresence,
}: PointCloudViewProps) {
  const { t } = useTranslation('translation')
  return (
    <div
      className="absolute inset-0"
      data-testid="point-cloud-view"
      aria-label={t('sensing.pointCloud.ariaLabel')}
    >
      <Canvas
        camera={{ position: [3.2, 1.8, 4.2], fov: 38, near: 0.1, far: 100 }}
        dpr={[1, 1.6]}
        gl={{ antialias: true, alpha: true }}
      >
        <color attach="background" args={['#e7f1ff']} />
        <fog attach="fog" args={['#e7f1ff', 4.8, 9]} />
        <gridHelper args={[7, 28, '#9db8ea', '#cbdcf4']} position={[0, -1.15, 0]} />
        <group rotation={[-Math.PI / 2, 0, 0]}>
          {hasSpatialData && <RadarPoints points={points} paused={paused} />}
          <TargetMarkers points={targetPoints} />
          {hasPresence && <BodyGuide breathingPhase={breathingPhase} />}
        </group>
        <OrbitControls
          key={viewResetKey}
          makeDefault
          enablePan={false}
          target={[0, 0, 0]}
          minDistance={3.2}
          maxDistance={7}
          autoRotate={!paused}
          autoRotateSpeed={0.32}
        />
      </Canvas>
    </div>
  )
}
