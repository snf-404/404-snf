import * as ProgressPrimitive from '@radix-ui/react-progress'
import * as React from 'react'

import { cn } from '@/lib/utils'

export const Progress = React.forwardRef<
  React.ElementRef<typeof ProgressPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof ProgressPrimitive.Root>
>(({ className, value = 0, ...props }, ref) => (
  <ProgressPrimitive.Root
    ref={ref}
    className={cn('relative h-1 overflow-hidden rounded-full bg-[rgba(70,100,160,.08)]', className)}
    {...props}
  >
    <ProgressPrimitive.Indicator
      className="size-full flex-1 bg-[#5278e8] transition-transform duration-700"
      style={{ transform: `translateX(-${String(100 - Math.min(100, Math.max(0, value ?? 0)))}%)` }}
    />
  </ProgressPrimitive.Root>
))
Progress.displayName = ProgressPrimitive.Root.displayName
