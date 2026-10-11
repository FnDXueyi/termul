import type { ComponentProps } from 'react'
import { SSHPopover } from '@/components/shell/SSHPopover'
import { ProjectSidebarColumn } from '@/layouts/workspace-layout/ProjectSidebarColumn'
import { isTauriContext } from '@/lib/tauri-runtime'
import { useSidebarVisible } from '@/stores/sidebar-store'

/**
 * Shell frame Rail area. Today's project sidebar column; it follows
 * `railExpanded` through the sidebar projection hook.
 *
 * Collapsed (desktop) it also shows the SSH popover in a minimal column. S23's
 * `RailExpanded`/`RailCollapsed` replace this shim and mount
 * `SSHResizableSection` and `SSHPopover` directly.
 */
export function ShellRail(props: ComponentProps<typeof ProjectSidebarColumn>): React.JSX.Element {
  const isSidebarVisible = useSidebarVisible()
  return (
    <>
      <ProjectSidebarColumn {...props} />
      {!isSidebarVisible && isTauriContext() && (
        <div className="flex w-12 shrink-0 flex-col items-center pt-1">
          <SSHPopover
            onSSHConnect={props.onSSHConnect}
            onSelectProfile={props.onSelectSSHProfile}
            activeProfileId={props.activeSSHProfileId}
          />
        </div>
      )}
    </>
  )
}
