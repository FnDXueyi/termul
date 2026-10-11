import { GitBranchPicker } from '@/components/GitBranchPicker'
import { Folder, FolderGit2, GitBranch } from '@/components/icons'
import { useChatIsolationContext } from '@/hooks/use-chat-isolation-context'
import { workingCopyOf } from '@/lib/working-copy'
import { reconcileProjectWorktrees } from '@/lib/worktree-reconciler'
import type { AcpSession } from '@/stores/acp-store'
import { useProjectStore } from '@/stores/project-store'

/**
 * Context strip under the composer once a chat has begun (desktop): the
 * working copy as plain text (`Worktree` or `Current checkout`, locked after
 * the first message) and the branch as today's `GitBranchPicker` at the chat's
 * cwd. A project without a git branch to show keeps the read-only text strip.
 */
export function ChatContextStrip({
  session
}: {
  session: Pick<AcpSession, 'projectId' | 'cwd' | 'worktreePath' | 'worktreeBranch'>
}): React.JSX.Element {
  const { projectId, cwd, worktreePath, worktreeBranch } = session
  const { isWorktree, isolationModeTitle, isolationBranch, isDetachedHead } =
    useChatIsolationContext({ projectId, worktreePath, worktreeBranch })
  const project = useProjectStore((s) => s.projects.find((p) => p.id === projectId))

  // A worktree chat prefers the listed worktree's branch (the reconciler keeps
  // it fresh after a checkout) over the branch recorded at launch.
  const wc = isWorktree ? workingCopyOf(cwd, project) : null
  const listedBranch =
    wc?.state === 'resolved' && wc.kind === 'worktree'
      ? project?.worktrees?.find((w) => w.id === wc.worktreeId)?.branch
      : undefined
  const branch = (isWorktree ? listedBranch : undefined) || isolationBranch

  const handleBranchChanged = (next: string): void => {
    if (!projectId) return
    if (isWorktree) void reconcileProjectWorktrees(projectId)
    else useProjectStore.getState().updateProject(projectId, { gitBranch: next })
  }

  const showBranch = Boolean(branch) || isDetachedHead
  return (
    <div
      data-chat-composer-context-strip="true"
      className="relative z-0 mx-auto -mt-4 flex w-[calc(100%-2.75rem)] min-w-0 items-center gap-2 rounded-2xl border border-t-0 border-border/60 bg-card/60 px-2 pb-1 pt-5 text-xs text-muted-foreground"
    >
      <span className="inline-flex shrink-0 items-center gap-1.5 px-2.5" title={isolationModeTitle}>
        {isWorktree ? (
          <FolderGit2 size={13} className="shrink-0" aria-hidden="true" />
        ) : (
          <Folder size={13} className="shrink-0" aria-hidden="true" />
        )}
        <span className="sr-only">Workspace: </span>
        {isWorktree ? 'Worktree' : 'Current checkout'}
      </span>
      {showBranch &&
        (projectId ? (
          <div className="ml-auto flex min-w-0 justify-end">
            <GitBranchPicker
              variant="strip"
              repoPath={cwd}
              currentBranch={branch}
              projectId={projectId}
              onBranchChanged={handleBranchChanged}
            />
          </div>
        ) : (
          <span
            className="ml-auto inline-flex min-w-0 items-center justify-end gap-1.5 px-2.5"
            title={branch ?? 'HEAD is not on a branch'}
          >
            <GitBranch size={13} className="shrink-0" aria-hidden="true" />
            <span className="sr-only">Branch: </span>
            <span className="truncate">{branch ?? 'Detached HEAD'}</span>
          </span>
        ))}
    </div>
  )
}
