import type { ComponentProps } from 'react'
import { ShellChangesColumn } from '@/components/shell/ShellChangesColumn'
import { ExplorerColumn } from '@/layouts/workspace-layout/ExplorerColumn'

type ShellSidePanelProps = Omit<ComponentProps<typeof ExplorerColumn>, 'side'>

/**
 * Shell frame Side panel area. Today's explorer column, placed left of the
 * main area; it follows `sidePanelVisible` through the explorer projection hook.
 * The Changes panel (with the docked Graph) sits beside it.
 */
export function ShellSidePanel(props: ShellSidePanelProps): React.JSX.Element {
  return (
    <>
      <ExplorerColumn {...props} side="left" />
      <ShellChangesColumn activeProject={props.activeProject} ssh={props.ssh} />
    </>
  )
}
