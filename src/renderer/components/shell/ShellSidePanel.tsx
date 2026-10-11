import type { ComponentProps } from 'react'
import { SidePanelColumn } from '@/components/shell/SidePanelColumn'
import { ExplorerColumn } from '@/layouts/workspace-layout/ExplorerColumn'

type ShellSidePanelProps = Omit<ComponentProps<typeof ExplorerColumn>, 'side' | 'sidePanel'>

/**
 * Shell frame Side panel area, placed left of the main area. `ExplorerColumn`
 * stays the outer shell (reveal motion, visibility, SSH browser, web edge
 * toggle; it follows `sidePanelVisible` through the explorer projection hook)
 * and the Column tabs panel is injected through its `sidePanel` slot.
 */
export function ShellSidePanel(props: ShellSidePanelProps): React.JSX.Element {
  return (
    <ExplorerColumn
      {...props}
      side="left"
      sidePanel={
        props.activeProject ? <SidePanelColumn activeProject={props.activeProject} /> : undefined
      }
    />
  )
}
