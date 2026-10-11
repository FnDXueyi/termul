import type { ComponentProps } from 'react'
import { ExplorerColumn } from '@/layouts/workspace-layout/ExplorerColumn'

type ShellSidePanelProps = Omit<ComponentProps<typeof ExplorerColumn>, 'side'>

/**
 * Shell frame Side panel area. Today's explorer column, placed left of the
 * main area; it follows `sidePanelVisible` through the explorer projection hook.
 */
export function ShellSidePanel(props: ShellSidePanelProps): React.JSX.Element {
  return <ExplorerColumn {...props} side="left" />
}
