import { cva, type VariantProps } from 'class-variance-authority'
import type { HTMLAttributes } from 'react'

import { cn } from '@/lib/utils'

const badgeVariants = cva('inline-flex items-center rounded-full border font-medium', {
  variants: {
    variant: {
      default: 'border-transparent bg-[#5278e8] text-white',
      secondary: 'border-[rgba(82,120,232,.2)] bg-[#5278e8]/[.07] text-[#5278e8]',
      outline: 'border-[rgba(70,100,160,.12)] text-[#7c8aa2]',
      destructive: 'border-red-500/20 bg-red-500/5 text-red-600',
    },
  },
  defaultVariants: { variant: 'default' },
})

export interface BadgeProps
  extends HTMLAttributes<HTMLDivElement>, VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />
}
