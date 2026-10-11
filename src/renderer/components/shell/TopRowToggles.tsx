import { Columns3, PanelLeft } from '@/components/icons'
import { RAIL_TOGGLE_LABEL, useRailToggle } from '@/components/shell/use-rail-toggle'
import {
  SIDE_PANEL_TOGGLE_LABEL,
  useSidePanelToggle
} from '@/components/shell/use-side-panel-toggle'
import { FOCUS_RING_CLASS, PANEL_ICON_BUTTON_CLASS } from '@/components/ui/panel-styles'
import { isTauriContext } from '@/lib/tauri-runtime'
import { cn } from '@/lib/utils'

/** Desktop: the titlebar toggle look (full-height, 16px glyph, hover wash). */
const DESKTOP_TOGGLE_CLASS = `inline-flex h-full cursor-pointer items-center px-3 transition-colors duration-150 ease-out hover:bg-foreground/[0.03] ${FOCUS_RING_CLASS} focus-visible:ring-inset`

interface ToggleProps {
  label: string
  pressed: boolean
  onClick: () => void
  icon: typeof PanelLeft
}

function TopRowToggle({ label, pressed, onClick, icon: Icon }: ToggleProps): React.JSX.Element {
  const desktop = isTauriContext()
  return (
    <button
      type="button"
      onClick={(event) => {
        event.stopPropagation()
        onClick()
      }}
      className={desktop ? DESKTOP_TOGGLE_CLASS : PANEL_ICON_BUTTON_CLASS}
      title={label}
      aria-label={label}
      aria-pressed={pressed}
    >
      <Icon
        size={desktop ? 16 : 14}
        className={cn(pressed ? 'text-foreground' : 'text-muted-foreground')}
      />
    </button>
  )
}

/** Expands or collapses the Rail. */
export function RailToggleButton(): React.JSX.Element {
  const { expanded, toggle } = useRailToggle()
  return (
    <TopRowToggle label={RAIL_TOGGLE_LABEL} pressed={expanded} onClick={toggle} icon={PanelLeft} />
  )
}

/** Shows or hides the Side panel. */
export function SidePanelToggleButton(): React.JSX.Element {
  const { visible, toggle } = useSidePanelToggle()
  return (
    <TopRowToggle
      label={SIDE_PANEL_TOGGLE_LABEL}
      pressed={visible}
      onClick={toggle}
      icon={Columns3}
    />
  )
}
