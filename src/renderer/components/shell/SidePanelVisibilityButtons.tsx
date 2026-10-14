import { ChevronsLeft, ChevronsRight } from '@/components/icons'
import { withShortcut } from '@/components/shell/TopRowToggles'
import { useShortcutLabel } from '@/components/shell/use-shortcut-label'
import { useSidePanelToggle } from '@/components/shell/use-side-panel-toggle'
import { PANEL_ICON_BUTTON_CLASS } from '@/components/ui/panel-styles'
import { useFileExplorerVisible } from '@/stores/file-explorer-store'
import { useActiveProject } from '@/stores/project-store'

export const HIDE_SIDE_PANEL_LABEL = 'Hide side panel'
export const SHOW_SIDE_PANEL_LABEL = 'Show side panel'

/**
 * Hides the Side panel. Lives in the panel's own header, so it acts on the
 * panel it sits in. Same persisted path as Ctrl+B.
 */
export function HideSidePanelButton(): React.JSX.Element {
  const { toggle } = useSidePanelToggle()
  const shortcut = useShortcutLabel('toggleFileExplorer')
  return (
    <button
      type="button"
      onClick={toggle}
      className={PANEL_ICON_BUTTON_CLASS}
      title={withShortcut(HIDE_SIDE_PANEL_LABEL, shortcut)}
      aria-label={HIDE_SIDE_PANEL_LABEL}
    >
      <ChevronsLeft size={14} aria-hidden="true" />
    </button>
  )
}

/**
 * Shows the hidden Side panel. Rendered at the start of the left top-edge
 * pane's tab bar, where the panel edge was.
 */
export function ShowSidePanelButton(): React.JSX.Element {
  const { toggle } = useSidePanelToggle()
  const shortcut = useShortcutLabel('toggleFileExplorer')
  return (
    <button
      type="button"
      onClick={(event) => {
        event.stopPropagation()
        toggle()
      }}
      className={PANEL_ICON_BUTTON_CLASS}
      title={withShortcut(SHOW_SIDE_PANEL_LABEL, shortcut)}
      aria-label={SHOW_SIDE_PANEL_LABEL}
    >
      <ChevronsRight size={14} aria-hidden="true" />
    </button>
  )
}

/**
 * Start of the leftmost top-edge tab bar: the Show side panel button and a
 * hairline, while the panel is hidden and the active project has a folder to
 * show (the same condition the Side panel column uses). Renders nothing
 * otherwise.
 */
export function ShowSidePanelSlot(): React.JSX.Element | null {
  const visible = useFileExplorerVisible()
  const hasPath = Boolean(useActiveProject()?.path)
  if (visible || !hasPath) return null
  return (
    <div
      className="flex h-full shrink-0 items-center gap-1 pl-2"
      data-testid="show-side-panel-slot"
    >
      <ShowSidePanelButton />
      <span aria-hidden className="ml-1 h-5 w-px shrink-0 bg-border/60" />
    </div>
  )
}
