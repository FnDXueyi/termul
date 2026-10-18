import { forwardRef, useId } from 'react'
import { cn } from '@/lib/utils'

interface SettingsGroupProps extends Omit<React.HTMLAttributes<HTMLElement>, 'title'> {
  /** Group heading, sentence case. */
  title?: React.ReactNode
  /** Controls that act on the whole group (refresh, add), placed after the heading. */
  actions?: React.ReactNode
  /** Short note under the group. */
  footnote?: React.ReactNode
}

/**
 * A titled block of settings on a page. Put rows inside a {@link SettingsCard};
 * anything else (a filter, a notice, a list) can sit between them.
 */
export const SettingsGroup = forwardRef<HTMLElement, SettingsGroupProps>(function SettingsGroup(
  { title, actions, footnote, className, children, ...props },
  ref
) {
  const titleId = useId()
  return (
    <section
      ref={ref}
      aria-labelledby={title ? titleId : undefined}
      className={cn('flex flex-col gap-2', className)}
      {...props}
    >
      {(title || actions) && (
        <div className="flex min-h-7 flex-wrap items-center justify-between gap-x-3 gap-y-1 px-1">
          {title && (
            <h3 id={titleId} className="text-xs font-medium text-muted-foreground">
              {title}
            </h3>
          )}
          {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
        </div>
      )}
      {children}
      {footnote && <p className="px-1 text-xs leading-relaxed text-muted-foreground">{footnote}</p>}
    </section>
  )
})

/**
 * The card that holds a group's rows. Rows get hairlines between them, and
 * the card is the container that narrow-layout rules measure.
 */
export const SettingsCard = forwardRef<HTMLDivElement, React.HTMLAttributes<HTMLDivElement>>(
  function SettingsCard({ className, ...props }, ref) {
    return (
      <div
        ref={ref}
        className={cn(
          '@container flex flex-col divide-y divide-border overflow-hidden rounded-lg border border-border bg-card',
          className
        )}
        {...props}
      />
    )
  }
)
