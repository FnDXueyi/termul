import type { ComponentProps } from 'react'
import { SidePanelColumn } from '@/components/shell/SidePanelColumn'
import { TopRowToggleStrip } from '@/components/shell/TopRowToggleStrip'
import { useTopRow } from '@/components/shell/top-row-context'
import { ExplorerColumn } from '@/layouts/workspace-layout/ExplorerColumn'
import { useSidebarVisible } from '@/stores/sidebar-store'

type ShellSidePanelProps = Omit<ComponentProps<typeof ExplorerColumn>, 'side' | 'sidePanel'>

/**
 * Shell frame Side panel area, placed left of the main area. `ExplorerColumn`
 * stays the outer shell (reveal motion, visibility, SSH browser, web edge
 * toggle; it follows `sidePanelVisible` through the explorer projection hook)
 * and the Column tabs panel is injected through its `sidePanel` slot.
 *
 * Rail expanded: the column header is the Side panel's part of the top row.
 * Rail collapsed: a left stack puts the toggle strip above the column (the
 * strip stays when the column is hidden, so the toggles remain reachable).
 */
export function ShellSidePanel(props: ShellSidePanelProps): React.JSX.Element {
  const railExpanded = useSidebarVisible()
  const dragRegion = useTopRow()?.platform.dragRegions ?? false

  const column = (
    <ExplorerColumn
      {...props}
      side="left"
      sidePanel={
        props.activeProject ? (
          <SidePanelColumn
            activeProject={props.activeProject}
            dragRegion={railExpanded && dragRegion}
          />
        ) : undefined
      }
    />
  )

  if (railExpanded) return column

  return (
    <div className="flex h-full shrink-0 flex-col" data-testid="side-panel-stack">
      <TopRowToggleStrip />
      <div className="flex min-h-0 flex-1">{column}</div>
    </div>
  )
}
