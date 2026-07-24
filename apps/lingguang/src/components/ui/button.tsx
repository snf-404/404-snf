import { Slot } from '@radix-ui/react-slot'
import { cva, type VariantProps } from 'class-variance-authority'
import * as React from 'react'

import { cn } from '@/lib/utils'

const buttonVariants = cva(
  'inline-flex items-center justify-center whitespace-nowrap transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#5278e8]/35 disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default: 'bg-[#5278e8] text-white shadow-[0_8px_20px_rgba(82,120,232,.2)]',
        outline: 'border border-[rgba(70,100,160,.12)] bg-white/60 text-[#43516a]',
        ghost: 'bg-transparent text-[#43516a]',
        destructive: 'border border-red-500/25 bg-red-500/5 text-red-600',
        unstyled: '',
      },
      size: {
        default: 'h-10 rounded-xl px-4 text-[13px]',
        sm: 'h-9 rounded-[10px] px-3 text-xs',
        icon: 'size-9 rounded-full',
        unstyled: '',
      },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  },
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : 'button'
    return (
      <Comp ref={ref} className={cn(buttonVariants({ variant, size }), className)} {...props} />
    )
  },
)
Button.displayName = 'Button'

export { buttonVariants }
