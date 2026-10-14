import type { ComponentProps } from 'react'
import { SidePanelColumn } from '@/components/shell/SidePanelColumn'
import { useTopRow, useTopRowPlatform } from '@/components/shell/top-row-context'
import { ExplorerColumn } from '@/layouts/workspace-layout/ExplorerColumn'
import { useSidebarVisible } from '@/stores/sidebar-store'

type ShellSidePanelProps = Omit<ComponentProps<typeof ExplorerColumn>, 'sidePanel'>

/**
 * Shell frame Side panel area, placed left of the main area. `ExplorerColumn`
 * stays the outer shell (reveal motion, visibility, SSH browser, web edge
 * toggle; it follows `sidePanelVisible` through the explorer projection hook)
 * and the Column tabs panel is injected through its `sidePanel` slot.
 *
 * In both Rail states the column header is the Side panel's part of the top
 * row. With the Rail collapsed on macOS the column starts under the traffic
 * lights, so its header takes extra clearance. Nothing sits above the column,
 * so a hidden Side panel leaves no gap.
 */
export function ShellSidePanel(props: ShellSidePanelProps): React.JSX.Element {
  const railExpanded = useSidebarVisible()
  const dragRegion = useTopRow()?.platform.dragRegions ?? false
  const { trafficLights } = useTopRowPlatform()

  return (
    <ExplorerColumn
      {...props}
      sidePanel={
        props.activeProject ? (
          <SidePanelColumn
            activeProject={props.activeProject}
            dragRegion={dragRegion}
            trafficLightInset={!railExpanded && trafficLights}
          />
        ) : undefined
      }
    />
  )
}
