import { forwardRef } from 'react'
import { Label } from '@/components/ui/label'
import { cn } from '@/lib/utils'

/** Id of a row's description, for the control's `aria-describedby`. */
export function settingsDescriptionId(controlId: string): string {
  return `${controlId}-description`
}

interface SettingsRowProps extends Omit<React.HTMLAttributes<HTMLDivElement>, 'children'> {
  /** What the setting does, sentence case. Becomes the control's label. */
  label: React.ReactNode
  /** One or two lines of help. */
  description?: React.ReactNode
  /** Id of the control the label names. Without it the label is plain text. */
  htmlFor?: string
  /**
   * Id stem for the description when the row has no `htmlFor` (a stepper
   * group). The description id is `settingsDescriptionId(htmlFor ?? describes)`.
   */
  describes?: string
  /**
   * `responsive` (default) moves the control under the label when the card is
   * narrow (Select, stepper, swatches). `inline` keeps it on the right (Switch).
   */
  layout?: 'responsive' | 'inline'
  /** Indent under the row above: a setting that depends on it. */
  indent?: boolean
  /** Dims the label; the description should say why it is off. */
  disabled?: boolean
  children?: React.ReactNode
}

/**
 * One setting: label and description on the left, control on the right.
 * Put it inside a `SettingsCard`. A search jump marks the row with
 * `data-settings-target` for two seconds.
 */
export const SettingsRow = forwardRef<HTMLDivElement, SettingsRowProps>(function SettingsRow(
  {
    label,
    description,
    htmlFor,
    describes,
    layout = 'responsive',
    indent,
    disabled,
    className,
    children,
    ...props
  },
  ref
) {
  const descriptionTarget = htmlFor ?? describes
  const descriptionId = descriptionTarget ? settingsDescriptionId(descriptionTarget) : undefined
  return (
    <div
      ref={ref}
      data-settings-row=""
      className={cn(
        'flex scroll-mt-4 gap-x-6 gap-y-2.5 px-4 py-3 transition-[background-color,box-shadow] duration-150 ease-out',
        'data-[settings-target]:bg-foreground/[0.04] data-[settings-target]:shadow-[inset_2px_0_0_oklch(var(--foreground))]',
        layout === 'inline' ? 'items-center' : 'flex-col @[35rem]:flex-row @[35rem]:items-center',
        indent && 'pl-10',
        className
      )}
      {...props}
    >
      <div className="min-w-0 flex-1">
        {htmlFor ? (
          <Label
            htmlFor={htmlFor}
            className={cn(
              'text-sm font-medium',
              disabled ? 'text-muted-foreground' : 'text-foreground'
            )}
          >
            {label}
          </Label>
        ) : (
          <div
            className={cn(
              'text-sm font-medium',
              disabled ? 'text-muted-foreground' : 'text-foreground'
            )}
          >
            {label}
          </div>
        )}
        {description && (
          <p id={descriptionId} className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {children !== undefined && <div className="flex shrink-0 items-center gap-2">{children}</div>}
    </div>
  )
})
