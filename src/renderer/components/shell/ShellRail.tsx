import type { ComponentProps } from 'react'
import { ProjectSidebarColumn } from '@/layouts/workspace-layout/ProjectSidebarColumn'

/**
 * Shell frame Rail area. Today's project sidebar column; it follows
 * `railExpanded` through the sidebar projection hook.
 */
export function ShellRail(props: ComponentProps<typeof ProjectSidebarColumn>): React.JSX.Element {
  return <ProjectSidebarColumn {...props} />
}
