import { PanelLeftCollapse, PanelLeftExpand } from '@/components/icons'
import { railButtonClass } from '@/components/shell/rail-button'
import { RAIL_TOGGLE_LABEL, useRailToggle } from '@/components/shell/use-rail-toggle'
import { useShortcutLabel, withShortcut } from '@/components/shell/use-shortcut-label'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

/**
 * Expands or collapses the Rail. It sits in the Rail's bottom-left corner,
 * at the same point in both Rail states, so it can be clicked again without
 * moving the pointer. The glyph names the next action (collapse when
 * expanded, expand when collapsed); the accessible name and `aria-pressed`
 * stay stable. No keycap: pressed here means "expanded", not "you are here".
 */
export function RailToggleButton(): React.JSX.Element {
  const { expanded, toggle } = useRailToggle()
  const shortcut = useShortcutLabel('sidebarToggle')
  const Icon = expanded ? PanelLeftCollapse : PanelLeftExpand
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={(event) => {
            event.stopPropagation()
            toggle()
          }}
          className={railButtonClass(false)}
          aria-label={RAIL_TOGGLE_LABEL}
          aria-pressed={expanded}
          data-testid="rail-toggle"
        >
          <Icon size={16} aria-hidden="true" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="right" className="px-2.5 py-1.5 text-xs">
        <span className="text-popover-foreground">
          {withShortcut(expanded ? 'Collapse projects' : 'Expand projects', shortcut)}
        </span>
      </TooltipContent>
    </Tooltip>
  )
}
