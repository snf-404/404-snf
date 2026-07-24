import * as SwitchPrimitive from '@radix-ui/react-switch'
import * as React from 'react'

import { cn } from '@/lib/utils'

export const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitive.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitive.Root
    ref={ref}
    className={cn(
      'peer inline-flex h-7 w-[46px] shrink-0 cursor-pointer items-center rounded-full border border-transparent bg-[rgba(70,100,160,.1)] transition-colors data-[state=checked]:bg-[#5278e8] disabled:cursor-not-allowed disabled:opacity-50',
      className,
    )}
    {...props}
  >
    <SwitchPrimitive.Thumb className="pointer-events-none block size-[22px] translate-x-0.5 rounded-full bg-[rgba(70,100,160,.38)] shadow-sm transition-transform data-[state=checked]:translate-x-5 data-[state=checked]:bg-white" />
  </SwitchPrimitive.Root>
))
Switch.displayName = SwitchPrimitive.Root.displayName
