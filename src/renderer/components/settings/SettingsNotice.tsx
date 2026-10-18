import { forwardRef } from 'react'
import { AlertCircle, AlertTriangle, CheckCircle2, Info } from '@/components/icons'
import { cn } from '@/lib/utils'

export type NoticeTone = 'info' | 'warning' | 'danger' | 'success'

const TONE: Record<NoticeTone, { box: string; icon: string; Glyph: typeof Info }> = {
  info: { box: 'border-border bg-foreground/[0.03]', icon: 'text-muted-foreground', Glyph: Info },
  warning: { box: 'border-warning/20 bg-warning/10', icon: 'text-warning', Glyph: AlertTriangle },
  danger: {
    box: 'border-destructive/20 bg-destructive/10',
    icon: 'text-destructive',
    Glyph: AlertCircle
  },
  success: { box: 'border-success/20 bg-success/10', icon: 'text-success', Glyph: CheckCircle2 }
}

interface SettingsNoticeProps extends React.HTMLAttributes<HTMLDivElement> {
  tone?: NoticeTone
  /** A button that resolves the notice, e.g. Try again. */
  action?: React.ReactNode
}

/** A boxed message on a settings page. Errors must say how to recover. */
export const SettingsNotice = forwardRef<HTMLDivElement, SettingsNoticeProps>(
  function SettingsNotice({ tone = 'info', action, className, children, ...props }, ref) {
    const { box, icon: iconClass, Glyph } = TONE[tone]
    return (
      <div
        ref={ref}
        className={cn('flex items-start gap-2.5 rounded-lg border px-3 py-2.5', box, className)}
        {...props}
      >
        <span aria-hidden="true" className={cn('mt-0.5 shrink-0', iconClass)}>
          <Glyph size={14} />
        </span>
        <div className="min-w-0 flex-1 text-xs leading-relaxed text-secondary-foreground">
          {children}
        </div>
        {action && <div className="shrink-0 self-center">{action}</div>}
      </div>
    )
  }
)
