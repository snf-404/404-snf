import { motion } from 'motion/react'

export type MatState =
  | 'work'
  | 'fatigue'
  | 'nudge'
  | 'cradle-forming'
  | 'cradle-stable'
  | 'wrap-forming'
  | 'wrap-active'
  | 'resetting'
  | 'safe-flat'
  | 'error'

interface DeskMatSVGProps {
  state: MatState
}

const C = {
  matLight: '#c4dff4',
  matMid: '#9bbdd8',
  matDeep: '#7aa8c8',
  matInner: '#688fb0',
  warmWhite: '#f0ece4',
  edgeShadow: 'rgba(0,0,0,0.28)',
  glowIce: 'rgba(130,210,255,0.5)',
  glowBrand: 'rgba(60,130,255,0.4)',
  glowWarm: 'rgba(255,180,100,0.3)',
  glowRed: 'rgba(200,50,70,0.4)',
}

// ViewBox: 0 0 320 190
// Mat fold lines: left near=(110,150) far=(104,83); right near=(210,150) far=(200,83)

function wingPath(side: 'left' | 'right', deg: number): string {
  const rad = (deg * Math.PI) / 180
  const c = Math.cos(rad)
  const s = Math.sin(rad)
  const ev = 0.374 // sin(22°) elevation foreshortening

  if (side === 'left') {
    const nx = 110 - c * 90
    const ny = 150 - s * 90 * ev
    const fx = 104 - c * 69
    const fy = 78 - s * 69 * ev
    return `M 110 150 L ${nx.toFixed(1)} ${ny.toFixed(1)} L ${fx.toFixed(1)} ${fy.toFixed(1)} L 104 83 Z`
  } else {
    const nx = 210 + c * 90
    const ny = 150 - s * 90 * ev
    const fx = 200 + c * 85
    const fy = 78 - s * 85 * ev
    return `M 210 150 L ${nx.toFixed(1)} ${ny.toFixed(1)} L ${fx.toFixed(1)} ${fy.toFixed(1)} L 200 83 Z`
  }
}

const CENTER = 'M 110 150 L 210 150 L 200 83 L 104 83 Z'
const FLAT = 'M 20 150 L 300 150 L 285 78 L 35 78 Z'
const FLAT_EDGE = 'M 20 150 L 300 150 L 300 161 L 20 161 Z'

export function DeskMatSVG({ state }: DeskMatSVGProps) {
  const isFlat =
    state === 'work' || state === 'fatigue' || state === 'safe-flat' || state === 'error'

  return (
    <svg
      viewBox="0 0 320 190"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      style={{ width: '100%', height: '100%', overflow: 'visible' }}
    >
      <defs>
        <linearGradient id="matGrad" x1="0" y1="0" x2="0.3" y2="1">
          <stop offset="0%" stopColor={C.matLight} />
          <stop offset="100%" stopColor={C.matDeep} />
        </linearGradient>
        <linearGradient id="matGradWing" x1="0" y1="0" x2="0.5" y2="1">
          <stop offset="0%" stopColor={C.matMid} />
          <stop offset="100%" stopColor={C.matInner} />
        </linearGradient>
        <linearGradient id="matGradCenter" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={C.matDeep} />
          <stop offset="100%" stopColor={C.matInner} />
        </linearGradient>
        <radialGradient id="glowCenter" cx="50%" cy="60%" r="50%">
          <stop offset="0%" stopColor="rgba(120,200,255,0.6)" />
          <stop offset="100%" stopColor="rgba(120,200,255,0)" />
        </radialGradient>
        <radialGradient id="glowFull" cx="50%" cy="55%" r="55%">
          <stop offset="0%" stopColor="rgba(90,170,240,0.4)" />
          <stop offset="100%" stopColor="rgba(90,170,240,0)" />
        </radialGradient>
        <radialGradient id="glowWarn" cx="50%" cy="55%" r="55%">
          <stop offset="0%" stopColor="rgba(255,150,80,0.35)" />
          <stop offset="100%" stopColor="rgba(255,150,80,0)" />
        </radialGradient>
        <radialGradient id="glowErr" cx="50%" cy="55%" r="55%">
          <stop offset="0%" stopColor="rgba(220,50,70,0.4)" />
          <stop offset="100%" stopColor="rgba(220,50,70,0)" />
        </radialGradient>
        <radialGradient id="glowWrap" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="rgba(60,140,240,0.35)" />
          <stop offset="100%" stopColor="rgba(60,140,240,0)" />
        </radialGradient>
        <filter id="softBlur" x="-30%" y="-30%" width="160%" height="160%">
          <feGaussianBlur stdDeviation="5" />
        </filter>
        <filter id="innerGlow" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation="3" result="blur" />
          <feComposite in="SourceGraphic" in2="blur" operator="over" />
        </filter>
        <linearGradient id="surfaceSheen" x1="0.1" y1="0" x2="0.9" y2="1">
          <stop offset="0%" stopColor="rgba(255,255,255,0.22)" />
          <stop offset="45%" stopColor="rgba(255,255,255,0.04)" />
          <stop offset="100%" stopColor="rgba(255,255,255,0)" />
        </linearGradient>
      </defs>

      {/* Drop shadow */}
      <ellipse cx="162" cy="178" rx="148" ry="9" fill="rgba(0,0,0,0.35)" filter="url(#softBlur)" />

      {/* ── FLAT STATES ── */}
      {isFlat && (
        <>
          {/* Main flat surface */}
          <path d={FLAT} fill="url(#matGrad)" />
          {/* Surface sheen */}
          <path d={FLAT} fill="url(#surfaceSheen)" />
          {/* Subtle contour lines */}
          <path
            d="M 55 130 Q 160 126 265 118"
            stroke="rgba(255,255,255,0.10)"
            strokeWidth="1"
            fill="none"
          />
          <path
            d="M 70 108 Q 160 104 250 97"
            stroke="rgba(255,255,255,0.07)"
            strokeWidth="1"
            fill="none"
          />
          {/* Warm white front edge */}
          <path d={FLAT_EDGE} fill={C.warmWhite} />
          <path d="M 20 161 L 300 161 L 300 165 L 20 165 Z" fill={C.edgeShadow} />

          {/* State-specific glows */}
          {state === 'work' && (
            <motion.ellipse
              cx="160"
              cy="120"
              rx="130"
              ry="50"
              fill="url(#glowCenter)"
              animate={{ opacity: [0.3, 0.55, 0.3] }}
              transition={{ duration: 10, repeat: Infinity, ease: 'easeInOut' }}
            />
          )}
          {state === 'fatigue' && (
            <>
              <motion.ellipse
                cx="160"
                cy="120"
                rx="130"
                ry="50"
                fill="url(#glowWarn)"
                animate={{ opacity: [0.4, 0.7, 0.4] }}
                transition={{ duration: 6, repeat: Infinity, ease: 'easeInOut' }}
              />
              {/* Edge warm tint */}
              <path d="M 35 78 L 20 150 L 28 150 L 44 78 Z" fill="rgba(255,160,80,0.15)" />
              <path d="M 285 78 L 300 150 L 291 150 L 276 78 Z" fill="rgba(255,160,80,0.15)" />
            </>
          )}
          {state === 'safe-flat' && (
            <motion.ellipse
              cx="160"
              cy="120"
              rx="120"
              ry="44"
              fill="url(#glowFull)"
              animate={{ opacity: [0.6, 1, 0.6] }}
              transition={{ duration: 1.5, repeat: 2, ease: 'easeOut' }}
            />
          )}
          {state === 'error' && (
            <motion.ellipse
              cx="160"
              cy="120"
              rx="130"
              ry="50"
              fill="url(#glowErr)"
              animate={{ opacity: [0.5, 0.8, 0.5] }}
              transition={{ duration: 3, repeat: Infinity, ease: 'easeInOut' }}
            />
          )}
        </>
      )}

      {/* ── NUDGE ── */}
      {state === 'nudge' && (
        <>
          <path d={FLAT} fill="url(#matGrad)" />
          <path d={FLAT} fill="url(#surfaceSheen)" />
          {/* Bump area in center-front */}
          <motion.ellipse
            cx="162"
            cy="140"
            rx="72"
            ry="18"
            fill="rgba(140,218,255,0.32)"
            animate={{ opacity: [0.32, 0.55, 0.32] }}
            transition={{ duration: 4, repeat: Infinity, ease: 'easeInOut' }}
          />
          {/* Contour rings showing elevation */}
          <ellipse
            cx="162"
            cy="140"
            rx="68"
            ry="15"
            stroke="rgba(160,230,255,0.45)"
            strokeWidth="1"
            fill="none"
          />
          <ellipse
            cx="162"
            cy="141"
            rx="50"
            ry="11"
            stroke="rgba(160,230,255,0.32)"
            strokeWidth="0.8"
            fill="none"
          />
          <ellipse
            cx="162"
            cy="142"
            rx="33"
            ry="7"
            stroke="rgba(160,230,255,0.22)"
            strokeWidth="0.6"
            fill="none"
          />
          {/* Warm front edge */}
          <path d={FLAT_EDGE} fill={C.warmWhite} />
          <path d="M 20 161 L 300 161 L 300 165 L 20 165 Z" fill={C.edgeShadow} />
          {/* Ice glow */}
          <motion.ellipse
            cx="162"
            cy="135"
            rx="90"
            ry="30"
            fill="rgba(80,180,255,0.18)"
            animate={{ opacity: [0.18, 0.35, 0.18] }}
            transition={{ duration: 5, repeat: Infinity, ease: 'easeInOut' }}
          />
        </>
      )}

      {/* ── CRADLE FORMING (35°) ── */}
      {state === 'cradle-forming' && (
        <>
          <path d={CENTER} fill="url(#matGradCenter)" />
          <path d={CENTER} fill="url(#surfaceSheen)" />
          <path d={wingPath('left', 35)} fill="url(#matGradWing)" />
          <path d={wingPath('right', 35)} fill="url(#matGradWing)" />
          {/* Wing sheen */}
          <path d={wingPath('left', 35)} fill="url(#surfaceSheen)" opacity="0.6" />
          <path d={wingPath('right', 35)} fill="url(#surfaceSheen)" opacity="0.6" />
          {/* Front edges on wings and center */}
          <line
            x1="110"
            y1="150"
            x2="210"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="8"
            strokeLinecap="round"
            opacity="0.9"
          />
          <line
            x1="20"
            y1="150"
            x2="110"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="4"
            strokeLinecap="round"
            opacity="0.7"
          />
          <line
            x1="210"
            y1="150"
            x2="300"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="4"
            strokeLinecap="round"
            opacity="0.7"
          />
          {/* Contour line on wings (mid-fold position) */}
          <path d="M 73 136 L 70 68" stroke="rgba(180,230,255,0.2)" strokeWidth="0.8" fill="none" />
          <path
            d="M 247 132 L 243 66"
            stroke="rgba(180,230,255,0.2)"
            strokeWidth="0.8"
            fill="none"
          />
          <motion.ellipse
            cx="162"
            cy="118"
            rx="95"
            ry="32"
            fill="rgba(80,165,240,0.2)"
            animate={{ opacity: [0.2, 0.4, 0.2] }}
            transition={{ duration: 6, repeat: Infinity, ease: 'easeInOut' }}
          />
        </>
      )}

      {/* ── CRADLE STABLE (60°) ── */}
      {state === 'cradle-stable' && (
        <>
          <path d={CENTER} fill="url(#matGradCenter)" />
          <path d={CENTER} fill="url(#surfaceSheen)" />
          <path d={wingPath('left', 60)} fill="url(#matGradWing)" />
          <path d={wingPath('right', 60)} fill="url(#matGradWing)" />
          <path d={wingPath('left', 60)} fill="url(#surfaceSheen)" opacity="0.5" />
          <path d={wingPath('right', 60)} fill="url(#surfaceSheen)" opacity="0.5" />
          {/* Bright rim on wing tops */}
          <path
            d={`M ${(110 - 0.5 * 90).toFixed(0)} ${(150 - 0.866 * 90 * 0.374).toFixed(0)} L ${(104 - 0.5 * 69).toFixed(0)} ${(78 - 0.866 * 69 * 0.374).toFixed(0)}`}
            stroke="rgba(200,240,255,0.55)"
            strokeWidth="1.5"
            fill="none"
          />
          <path
            d={`M ${(210 + 0.5 * 90).toFixed(0)} ${(150 - 0.866 * 90 * 0.374).toFixed(0)} L ${(200 + 0.5 * 85).toFixed(0)} ${(78 - 0.866 * 85 * 0.374).toFixed(0)}`}
            stroke="rgba(200,240,255,0.55)"
            strokeWidth="1.5"
            fill="none"
          />
          {/* Front edge center */}
          <line
            x1="110"
            y1="150"
            x2="210"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="9"
            strokeLinecap="round"
            opacity="0.95"
          />
          <line
            x1="20"
            y1="150"
            x2="110"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="5"
            strokeLinecap="round"
            opacity="0.6"
          />
          <line
            x1="210"
            y1="150"
            x2="300"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="5"
            strokeLinecap="round"
            opacity="0.6"
          />
          {/* Center bowl shadow */}
          <path d={CENTER} fill="rgba(0,0,0,0.12)" />
          {/* Ambient cradle glow */}
          <motion.ellipse
            cx="162"
            cy="118"
            rx="88"
            ry="30"
            fill="rgba(70,155,235,0.25)"
            animate={{ opacity: [0.25, 0.45, 0.25] }}
            transition={{ duration: 8, repeat: Infinity, ease: 'easeInOut' }}
          />
        </>
      )}

      {/* ── WRAP FORMING (80°) ── */}
      {state === 'wrap-forming' && (
        <>
          <path d={CENTER} fill="url(#matGradCenter)" />
          <path d={wingPath('left', 80)} fill="url(#matGradWing)" />
          <path d={wingPath('right', 80)} fill="url(#matGradWing)" />
          {/* Inner surface sheen */}
          <path d={wingPath('left', 80)} fill="rgba(140,200,240,0.15)" />
          <path d={wingPath('right', 80)} fill="rgba(140,200,240,0.15)" />
          {/* Wing top rim lines */}
          {(() => {
            const lnx = (110 - 0.174 * 90).toFixed(1)
            const lny = (150 - 0.985 * 90 * 0.374).toFixed(1)
            const lfx = (104 - 0.174 * 69).toFixed(1)
            const lfy = (78 - 0.985 * 69 * 0.374).toFixed(1)
            const rnx = (210 + 0.174 * 90).toFixed(1)
            const rny = (150 - 0.985 * 90 * 0.374).toFixed(1)
            const rfx = (200 + 0.174 * 85).toFixed(1)
            const rfy = (78 - 0.985 * 85 * 0.374).toFixed(1)
            return (
              <>
                <path
                  d={`M ${lnx} ${lny} L ${lfx} ${lfy}`}
                  stroke="rgba(200,240,255,0.7)"
                  strokeWidth="1.5"
                  fill="none"
                />
                <path
                  d={`M ${rnx} ${rny} L ${rfx} ${rfy}`}
                  stroke="rgba(200,240,255,0.7)"
                  strokeWidth="1.5"
                  fill="none"
                />
                {/* Opening top edge */}
                <path
                  d={`M ${lnx} ${lny} L ${rnx} ${rny}`}
                  stroke="rgba(180,230,255,0.35)"
                  strokeWidth="1"
                  strokeDasharray="4 4"
                  fill="none"
                />
              </>
            )
          })()}
          {/* Front edge */}
          <line
            x1="110"
            y1="150"
            x2="210"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="10"
            strokeLinecap="round"
          />
          <line
            x1="20"
            y1="150"
            x2="110"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="5"
            strokeLinecap="round"
            opacity="0.45"
          />
          <line
            x1="210"
            y1="150"
            x2="300"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="5"
            strokeLinecap="round"
            opacity="0.45"
          />
          <motion.ellipse
            cx="162"
            cy="110"
            rx="68"
            ry="26"
            fill="rgba(50,130,230,0.3)"
            animate={{ opacity: [0.3, 0.55, 0.3] }}
            transition={{ duration: 9, repeat: Infinity, ease: 'easeInOut' }}
          />
        </>
      )}

      {/* ── WRAP ACTIVE (100°) ── */}
      {state === 'wrap-active' && (
        <>
          {/* Inner channel floor */}
          <path d={CENTER} fill="url(#matGradCenter)" />
          <path d={CENTER} fill="rgba(0,0,0,0.2)" />
          {/* Inner wing faces */}
          <path d={wingPath('left', 100)} fill="url(#matGradWing)" />
          <path d={wingPath('right', 100)} fill="url(#matGradWing)" />
          {/* Inner surface ambient */}
          <path d={wingPath('left', 100)} fill="rgba(120,190,240,0.18)" />
          <path d={wingPath('right', 100)} fill="rgba(120,190,240,0.18)" />
          {/* Wing rim highlights */}
          {(() => {
            const lnx = (110 + 0.174 * 90).toFixed(1)
            const lny = (150 - 0.985 * 90 * 0.374).toFixed(1)
            const lfx = (104 + 0.174 * 69).toFixed(1)
            const lfy = (78 - 0.985 * 69 * 0.374).toFixed(1)
            const rnx = (210 - 0.174 * 90).toFixed(1)
            const rny = (150 - 0.985 * 90 * 0.374).toFixed(1)
            const rfx = (200 - 0.174 * 85).toFixed(1)
            const rfy = (78 - 0.985 * 85 * 0.374).toFixed(1)
            return (
              <>
                <path
                  d={`M ${lnx} ${lny} L ${lfx} ${lfy}`}
                  stroke="rgba(210,245,255,0.8)"
                  strokeWidth="1.8"
                  fill="none"
                />
                <path
                  d={`M ${rnx} ${rny} L ${rfx} ${rfy}`}
                  stroke="rgba(210,245,255,0.8)"
                  strokeWidth="1.8"
                  fill="none"
                />
                {/* Opening between wings */}
                <path
                  d={`M ${lnx} ${lny} L 110 150 L 210 150 L ${rnx} ${rny} Z`}
                  fill="rgba(60,120,220,0.12)"
                  stroke="rgba(160,220,255,0.25)"
                  strokeWidth="0.8"
                />
              </>
            )
          })()}
          {/* Deep wrap ambient glow */}
          <motion.ellipse
            cx="162"
            cy="105"
            rx="55"
            ry="22"
            fill="rgba(40,110,220,0.4)"
            animate={{ opacity: [0.4, 0.65, 0.4] }}
            transition={{ duration: 11, repeat: Infinity, ease: 'easeInOut' }}
          />
          {/* Front edge visible in center opening */}
          <line
            x1="110"
            y1="150"
            x2="210"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="10"
            strokeLinecap="round"
            opacity="0.9"
          />
        </>
      )}

      {/* ── RESETTING (35° → flat transition) ── */}
      {state === 'resetting' && (
        <>
          <path d={CENTER} fill="url(#matGradCenter)" />
          <path d={wingPath('left', 35)} fill="url(#matGradWing)" opacity="0.7" />
          <path d={wingPath('right', 35)} fill="url(#matGradWing)" opacity="0.7" />
          {/* Fading glow showing retraction */}
          <motion.ellipse
            cx="162"
            cy="118"
            rx="110"
            ry="38"
            fill="rgba(80,160,230,0.2)"
            animate={{ opacity: [0.2, 0.05, 0.2] }}
            transition={{ duration: 3, repeat: Infinity, ease: 'easeInOut' }}
          />
          {/* Front edge */}
          <line
            x1="110"
            y1="150"
            x2="210"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="9"
            strokeLinecap="round"
          />
          <line
            x1="20"
            y1="150"
            x2="110"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="4"
            strokeLinecap="round"
            opacity="0.5"
          />
          <line
            x1="210"
            y1="150"
            x2="300"
            y2="150"
            stroke={C.warmWhite}
            strokeWidth="4"
            strokeLinecap="round"
            opacity="0.5"
          />
        </>
      )}
    </svg>
  )
}
