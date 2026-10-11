import { useEffect, useRef } from 'react'
import { useGitStatusStore } from '@/stores/git-status-store'
import { useProjectStore } from '@/stores/project-store'
import type { Project } from '@/types/project'

/**
 * Auto-detect the active project's current git branch and seed
 * `project.gitBranch` with the real HEAD branch — instead of showing a stale
 * hardcoded 'main' in the status bar before any terminal has reported a branch
 * (or when the project has no active terminal at all).
 *
 * Runs on active-project change. Skips projects that are not git repos or have
 * no path. Goes through the git status store (`fetchCommitContext`, the single
 * git cache), which is transport-neutral (desktop invoke / same-origin server
 * HTTP), so the status bar reflects the real branch on both desktop and the
 * termul-server web/remote client.
 *
 * A successful probe (a resolvable HEAD commit) also stamps `isGitRepo: true`.
 * On desktop the worktree reconciler already flips that flag, but on web it
 * early-returns (`isTauriContext()` gate) and `ProjectSummary` carries no git
 * fields — this success path is what lets the web launcher's worktree
 * isolation strip (`canUseWorktree`) appear for a git-repo project.
 *
 * A detached HEAD (`branch: null`) clears the stored `project.gitBranch` so the
 * status bar shows 'detached' truthfully rather than a stale prior branch.
 * Best-effort: a fetch failure leaves the existing (or absent) value alone;
 * the manual GitBranchPicker switch path still updates `gitBranch`, and the
 * per-terminal git branch events (useGitBranch) take precedence in the status
 * bar once a terminal reports its branch.
 */
export function useProjectGitBranch(): void {
  const activeProjectId = useProjectStore((state) => state.activeProjectId)
  const updateProject = useProjectStore((state) => state.updateProject)
  // Monotonic request token so a fast A→B→A switch cannot let a slow earlier
  // fetch stampede over a newer result: a stale token is superseded even when
  // the project id repeats.
  const tokenRef = useRef(0)

  useEffect(() => {
    if (!activeProjectId) return

    const project: Project | undefined = useProjectStore
      .getState()
      .projects.find((p) => p.id === activeProjectId)
    // No path or known non-git → nothing to detect. `isGitRepo` is undefined
    // for fresh projects (not yet reconciled) — still attempt detection so a
    // repo is detected on first activation rather than only after the worktree
    // reconciler flips the flag.
    if (!project?.path || project.isGitRepo === false) return

    const token = ++tokenRef.current
    const path = project.path
    let cancelled = false

    // The store stores and logs a failed fetch (`[git-status]`) and resolves
    // `null`, so there is nothing to catch or log here.
    void useGitStatusStore
      .getState()
      .fetchCommitContext(path)
      .then((context) => {
        if (cancelled || token !== tokenRef.current) return
        // Not a git repo, git missing, or fetch failed — leave the existing
        // value alone. The worktree reconciler handles isGitRepo separately.
        if (!context) return
        // branch is null on a detached HEAD / no-branch state — clear the
        // stored value so the status bar renders 'detached' truthfully
        // instead of a stale prior branch.
        updateProject(activeProjectId, {
          gitBranch: context.branch ?? undefined,
          // `hasHead: true` (HEAD resolves) means the path IS a git repo —
          // stamp it so capability gates (worktree isolation picker) hold
          // the same truth on web as the desktop reconciler provides. The
          // route still succeeds with `hasHead: false` for a non-repo path,
          // so the flag must NOT be stamped then.
          ...(context.hasHead ? { isGitRepo: true } : {})
        })
      })

    return () => {
      cancelled = true
    }
  }, [activeProjectId, updateProject])
}
