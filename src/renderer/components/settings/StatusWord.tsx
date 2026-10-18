import { forwardRef } from 'react'
import { Spinner } from '@/components/ui/spinner'
import { cn } from '@/lib/utils'

export type StatusTone = 'success' | 'warning' | 'danger' | 'muted'

const DOT: Record<StatusTone, string> = {
  success: 'bg-success',
  warning: 'bg-warning',
  danger: 'bg-destructive',
  muted: 'bg-muted-foreground/60'
}

const TEXT: Record<StatusTone, string> = {
  success: 'text-success',
  warning: 'text-warning',
  danger: 'text-destructive',
  muted: 'text-muted-foreground'
}

interface StatusWordProps extends React.HTMLAttributes<HTMLSpanElement> {
  tone: StatusTone
  /** Shows a spinner instead of the dot, for "Checking…" states. */
  pending?: boolean
}

/** A dot and a word. The word carries the meaning; the color only repeats it. */
export const StatusWord = forwardRef<HTMLSpanElement, StatusWordProps>(function StatusWord(
  { tone, pending, className, children, ...props },
  ref
) {
  return (
    <span
      ref={ref}
      className={cn('inline-flex shrink-0 items-center gap-1.5 text-xs', TEXT[tone], className)}
      {...props}
    >
      {pending ? (
        <Spinner size={12} decorative />
      ) : (
        <span aria-hidden="true" className={cn('size-1.5 shrink-0 rounded-full', DOT[tone])} />
      )}
      {children}
    </span>
  )
})
