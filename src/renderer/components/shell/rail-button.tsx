import { QUIET_ICON_BUTTON_CLASS } from '@/components/ui/panel-styles'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

/**
 * 36px rail action. Rest = muted icon; hover = 3% foreground wash; the
 * pressed / open view takes the `.keycap` surface. Disabled icons sit at 40%
 * with no hover.
 */
export function railButtonClass(active: boolean): string {
  return cn(
    QUIET_ICON_BUTTON_CLASS,
    'relative flex size-9 shrink-0 rounded-lg disabled:pointer-events-none disabled:opacity-40',
    active && 'keycap text-foreground'
  )
}

interface RailButtonProps {
  label: string
  /** Muted second line in the tooltip (e.g. why the action is disabled). */
  hint?: string
  'aria-label': string
  disabled?: boolean
  /** Sets `aria-pressed` and the keycap "you are here" surface. */
  pressed?: boolean
  badge?: React.ReactNode
  /** Runs after the click stops propagating. */
  onClick: () => void
  children: React.ReactNode
}

export function RailButton({
  label,
  hint,
  'aria-label': ariaLabel,
  disabled = false,
  pressed,
  badge,
  onClick,
  children
}: RailButtonProps): React.JSX.Element {
  const button = (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation()
        onClick()
      }}
      className={railButtonClass(pressed ?? false)}
      aria-label={ariaLabel}
      aria-pressed={pressed}
      aria-disabled={disabled || undefined}
      disabled={disabled}
    >
      {children}
      {badge}
    </button>
  )

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {/* A disabled button gets no pointer events, so the wrapper carries
            the tooltip that explains why the action is off. */}
        {disabled ? <span className="inline-flex">{button}</span> : button}
      </TooltipTrigger>
      <TooltipContent side="right" className="flex flex-col gap-0.5 px-2.5 py-1.5 text-xs">
        <span className="text-popover-foreground">{label}</span>
        {hint && <span className="text-muted-foreground">{hint}</span>}
      </TooltipContent>
    </Tooltip>
  )
}
