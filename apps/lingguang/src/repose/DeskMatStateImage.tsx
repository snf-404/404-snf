import { AnimatePresence, motion } from 'motion/react'

import cradleFormingImage from '@/assets/repose-states/cradle-forming.png'
import cradleStableImage from '@/assets/repose-states/cradle-stable.png'
import errorImage from '@/assets/repose-states/error.png'
import fatigueImage from '@/assets/repose-states/fatigue.png'
import nudgeImage from '@/assets/repose-states/nudge.png'
import resettingImage from '@/assets/repose-states/resetting.png'
import workImage from '@/assets/repose-states/work.png'
import wrapActiveImage from '@/assets/repose-states/wrap-active.png'
import wrapFormingImage from '@/assets/repose-states/wrap-forming.png'

export type MatVisualState =
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

const STATE_IMAGES: Record<MatVisualState, string> = {
  work: workImage,
  fatigue: fatigueImage,
  nudge: nudgeImage,
  'cradle-forming': cradleFormingImage,
  'cradle-stable': cradleStableImage,
  'wrap-forming': wrapFormingImage,
  'wrap-active': wrapActiveImage,
  resetting: resettingImage,
  'safe-flat': workImage,
  error: errorImage,
}

interface DeskMatStateImageProps {
  state: MatVisualState
  alt: string
  className?: string
}

export function DeskMatStateImage({ state, alt, className }: DeskMatStateImageProps) {
  return (
    <AnimatePresence mode="wait" initial={false}>
      <motion.img
        key={state}
        src={STATE_IMAGES[state]}
        alt={alt}
        draggable={false}
        className={className}
        initial={{ opacity: 0, scale: 1.02 }}
        animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0, scale: 0.98 }}
        transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
      />
    </AnimatePresence>
  )
}
