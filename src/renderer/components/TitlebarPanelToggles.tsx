import { toast } from 'sonner'
import { PanelLeft, PanelRight } from '@/components/icons'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { useUpdatePanelVisibility } from '@/hooks/use-app-settings'
import { useFileExplorerVisible } from '@/stores/file-explorer-store'
import { useSidebarVisible } from '@/stores/sidebar-store'

/** Hover wash, neutral inset focus ring and pointer shared by both toggle looks. */
const toggleButtonBaseClass = `inline-flex items-center cursor-pointer transition-colors duration-150 ease-out hover:bg-foreground/[0.03] ${FOCUS_RING_CLASS} focus-visible:ring-inset`

/**
 * Shared button style for panel-visibility toggles rendered inside a titlebar.
 *
 * Mirrors the window-control button metrics (`h-full px-3`, 16px icons matching
 * Minimize/Close) so the toggles visually belong to the titlebar strip.
 * Icon ink is foreground while the panel is visible, muted when hidden.
 */
const titlebarToggleButtonClass = `h-full px-3 ${toggleButtonBaseClass}`

/**
 * Neutral edge toggle shown beside a hidden panel on web (32px square).
 * Same states as the titlebar toggle.
 */
export const panelEdgeToggleButtonClass = `h-8 w-8 justify-center rounded-md ${toggleButtonBaseClass}`

/**
 * Marks an element non-draggable so buttons stay clickable inside a
 * `data-tauri-drag-region` strip. Shared by the shell Rail header and top row.
 */
export const titlebarNoDragStyle = { WebkitAppRegion: 'no-drag' } as React.CSSProperties

interface ToggleButtonProps {
  /** Overrides the default titlebar button metrics. */
  className?: string
}

interface PanelVisibilityToggleProps extends ToggleButtonProps {
  setting: 'sidebarVisible' | 'fileExplorerVisible'
  /** Lower-case panel name for labels and the error toast ("sidebar"). */
  panelName: string
  isVisible: boolean
  icon: typeof PanelLeft
}

/**
 * Panel visibility toggle. Behavior contract preserved from the former
 * ActivityRail placement: persistence-aware update via
 * `useUpdatePanelVisibility`, error toast on failure, and accessible
 * pressed/label state.
 */
function PanelVisibilityToggle({
  setting,
  panelName,
  isVisible,
  icon: Icon,
  className = titlebarToggleButtonClass
}: PanelVisibilityToggleProps): React.JSX.Element {
  const updatePanelVisibility = useUpdatePanelVisibility()

  const handleClick = async (): Promise<void> => {
    try {
      await updatePanelVisibility(setting, !isVisible)
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : `Failed to update ${panelName} visibility`
      )
    }
  }

  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation()
        void handleClick()
      }}
      className={className}
      title={`Toggle ${panelName}`}
      aria-label={`${isVisible ? 'Hide' : 'Show'} ${panelName}`}
      aria-pressed={isVisible}
    >
      <Icon size={16} className={isVisible ? 'text-foreground' : 'text-muted-foreground'} />
    </button>
  )
}

/** Left-sidebar visibility toggle rendered in the titlebar. */
export function SidebarToggleButton({ className }: ToggleButtonProps): React.JSX.Element {
  const isVisible = useSidebarVisible()
  return (
    <PanelVisibilityToggle
      setting="sidebarVisible"
      panelName="sidebar"
      isVisible={isVisible}
      icon={PanelLeft}
      className={className}
    />
  )
}

/** Right-sidebar (file explorer) visibility toggle rendered in the titlebar. */
export function FileExplorerToggleButton({ className }: ToggleButtonProps): React.JSX.Element {
  const isVisible = useFileExplorerVisible()
  return (
    <PanelVisibilityToggle
      setting="fileExplorerVisible"
      panelName="file explorer"
      isVisible={isVisible}
      icon={PanelRight}
      className={className}
    />
  )
}
