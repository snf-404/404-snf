import { cva, type VariantProps } from 'class-variance-authority'
import * as React from 'react'

import { cn } from '@/lib/utils'

const alertVariants = cva('relative w-full rounded-[14px] border px-4 py-3 text-sm', {
  variants: {
    variant: {
      default: 'border-[rgba(70,100,160,.1)] bg-[rgba(70,100,160,.04)] text-[#43516a]',
      warning: 'border-amber-600/20 bg-amber-600/5 text-[#9a6e10]',
      destructive: 'border-red-500/20 bg-red-500/5 text-[#c43e3e]',
    },
  },
  defaultVariants: { variant: 'default' },
})

export interface AlertProps
  extends React.HTMLAttributes<HTMLDivElement>, VariantProps<typeof alertVariants> {}

export const Alert = React.forwardRef<HTMLDivElement, AlertProps>(
  ({ className, variant, ...props }, ref) => (
    <div ref={ref} role="alert" className={cn(alertVariants({ variant }), className)} {...props} />
  ),
)
Alert.displayName = 'Alert'
