import { ChangesPanel } from '@/components/shell/ChangesPanel'
import type { SSHWorkspaceState } from '@/layouts/workspace-layout/use-ssh-workspace'
import { pickedWorkingCopy } from '@/lib/working-copy'
import { useFileExplorerVisible } from '@/stores/file-explorer-store'
import { useSidePanelWidth, useWorktreePick } from '@/stores/shell-layout-store'
import type { Project } from '@/types/project'

interface ShellChangesColumnProps {
  activeProject: Project | undefined
  ssh: SSHWorkspaceState
}

/**
 * Side-panel mount of the Changes panel, beside the explorer. Shown while the
 * explorer is visible, the project has a folder and no SSH profile is active;
 * it shows the worktree picked in the switcher, else the project folder.
 */
export function ShellChangesColumn({
  activeProject,
  ssh
}: ShellChangesColumnProps): React.JSX.Element | null {
  const isExplorerVisible = useFileExplorerVisible()
  const sidePanelWidth = useSidePanelWidth()
  const pickedPath = useWorktreePick(activeProject?.id ?? '')

  const projectPath = activeProject?.path
  if (!isExplorerVisible || !projectPath || ssh.activeSSHProfile) return null

  const cwd = pickedWorkingCopy(activeProject, pickedPath)?.path ?? projectPath

  return (
    <div
      className="relative flex h-full min-w-0 flex-shrink-0 flex-col overflow-hidden rounded-xl bg-background text-foreground"
      style={{ width: sidePanelWidth }}
    >
      <ChangesPanel cwd={cwd} isVisible />
    </div>
  )
}
