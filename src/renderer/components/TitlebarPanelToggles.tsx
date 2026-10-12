import { toast } from 'sonner'
import { PanelRight } from '@/components/icons'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { useUpdatePanelVisibility } from '@/hooks/use-app-settings'
import { useFileExplorerVisible } from '@/stores/file-explorer-store'

/** Hover wash, neutral inset focus ring and pointer shared by the toggle looks. */
const toggleButtonBaseClass = `inline-flex items-center cursor-pointer transition-colors duration-150 ease-out hover:bg-foreground/[0.03] ${FOCUS_RING_CLASS} focus-visible:ring-inset`

/**
 * Neutral edge toggle shown beside a hidden panel on web (32px square).
 */
export const panelEdgeToggleButtonClass = `h-8 w-8 justify-center rounded-md ${toggleButtonBaseClass}`

/**
 * Marks an element non-draggable so buttons stay clickable inside a
 * `data-tauri-drag-region` strip. Shared by the shell Rail header and top row.
 */
export const titlebarNoDragStyle = { WebkitAppRegion: 'no-drag' } as React.CSSProperties

interface ToggleButtonProps {
  /** Overrides the default edge toggle metrics. */
  className?: string
}

/**
 * File explorer (Side panel) visibility toggle. Persistence-aware update via
 * `useUpdatePanelVisibility`, error toast on failure, and accessible
 * pressed/label state. Icon ink is foreground while the panel is visible,
 * muted when hidden.
 */
export function FileExplorerToggleButton({
  className = panelEdgeToggleButtonClass
}: ToggleButtonProps): React.JSX.Element {
  const isVisible = useFileExplorerVisible()
  const updatePanelVisibility = useUpdatePanelVisibility()

  const handleClick = async (): Promise<void> => {
    try {
      await updatePanelVisibility('fileExplorerVisible', !isVisible)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Failed to update side panel visibility')
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
      title="Show or hide side panel"
      aria-label={`${isVisible ? 'Hide' : 'Show'} side panel`}
      aria-pressed={isVisible}
    >
      <PanelRight size={16} className={isVisible ? 'text-foreground' : 'text-muted-foreground'} />
    </button>
  )
}
