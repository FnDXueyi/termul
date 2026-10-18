import type { SSHWorkspaceState } from '@/layouts/workspace-layout/use-ssh-workspace'
import type { Project } from '@/types/project'

/**
 * Which blocks the Side panel column shows. Both `false` means the column is
 * gone and the main card sits next to the Rail.
 */
export function getExplorerColumnBlocks({
  isExplorerVisible,
  activeProject,
  ssh,
  hasSidePanel
}: {
  isExplorerVisible: boolean
  activeProject: Project | undefined
  ssh: SSHWorkspaceState
  hasSidePanel: boolean
}): { showFileColumn: boolean; sftpSession: SSHWorkspaceState['activeSSHSession'] | null } {
  const { workspaceOpen, activeSSHSession } = ssh
  const showFileColumn = Boolean(
    isExplorerVisible && activeProject?.path && !(hasSidePanel && workspaceOpen)
  )
  // Remote files of the active, connected session tab.
  const sftpSession =
    workspaceOpen && isExplorerVisible && activeSSHSession?.status === 'connected'
      ? activeSSHSession
      : null
  return { showFileColumn, sftpSession }
}
