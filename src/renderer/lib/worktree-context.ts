/**
 * Worktree context utilities for terminal and file explorer integration.
 *
 * `getDefaultCwdForProject` resolves the main project root for new terminals
 * and ACP sessions; worktree-isolated chats override their cwd explicitly.
 * The remaining helpers power the worktree indicator and file-explorer root.
 */

import { activeWorktreeOf } from '@/lib/active-worktree'
import { worktreeApi } from '@/lib/api'
import { useProjectStore } from '@/stores/project-store'
import { useShellLayoutStore } from '@/stores/shell-layout-store'
import type { Project, Worktree } from '@/types/project'

/**
 * Get the main project root path (ignoring any active worktree).
 *
 * Skills are stored at `{project.path}/.agents/skills/`, which is gitignored
 * (`/.agents`) and excluded from worktree symlinks (`SYMLINK_EXCLUSION_LIST`).
 * A worktree path therefore has no `.agents/skills/`, so skill discovery must
 * always resolve against the main project root — never the worktree CWD.
 */
export function getProjectRootPath(projectId: string): string {
  const project = useProjectStore.getState().projects.find((p) => p.id === projectId)
  return project?.path ?? ''
}

/**
 * Get the default CWD for new terminals and ACP sessions — always the main
 * project root. The active-worktree binding does NOT redirect new sessions:
 * worktree-isolated agent chats override `launchCwd` explicitly (AgentLauncher
 * worktree-mode), and resumed sessions use `session.cwd` directly. Only the
 * worktree indicator/file-explorer root read the switcher pick (via
 * `getActiveWorktreeForProject`/`getActiveWorktreeRoot`), not this helper.
 */
export function getDefaultCwdForProject(projectId: string): string {
  const project = useProjectStore.getState().projects.find((p) => p.id === projectId)
  return project?.path ?? ''
}

/** The Workspace-switcher pick for a project, read from the shell-layout store. */
function pickOf(projectId: string): string | undefined {
  return useShellLayoutStore.getState().worktreePickByProject[projectId]
}

/**
 * Get the active worktree for a project, if any: the worktree the switcher
 * pick resolves to (a pruned or disabled pick resolves as the project folder).
 */
export function getActiveWorktreeForProject(projectId: string): Worktree | undefined {
  const project = useProjectStore.getState().projects.find((p) => p.id === projectId)
  return activeWorktreeOf(project, pickOf(projectId))
}

/**
 * Get a display-friendly context string for a terminal running in a worktree.
 * Format: "project-name / worktree-name" or "project-name / Root"
 */
export function getWorktreeContextLabel(project: Project): string {
  const worktree = activeWorktreeOf(project, pickOf(project.id))
  if (worktree) {
    return `${project.name} / ${worktree.name}`
  }
  return `${project.name} / Root`
}

/**
 * Get the worktree name for display in terminal tabs.
 * Returns the worktree branch name, or null if on project root.
 */
export function getWorktreeTabContext(projectId: string): string | null {
  const worktree = getActiveWorktreeForProject(projectId)
  return worktree?.name ?? null
}

/**
 * Get the active worktree root path for a project, if any.
 * Returns the worktree path if an active worktree is set, or null if on project root.
 * This is useful for the file explorer to root at the worktree path.
 */
export function getActiveWorktreeRoot(projectId: string): string | null {
  return getActiveWorktreeForProject(projectId)?.path ?? null
}

/**
 * Check if a path still exists on disk by attempting to verify it.
 * This is a synchronous check that can be used to detect stale worktree paths.
 * Note: Actual file system checks should be done async via the Rust backend.
 * TODO: Move staleness detection to useWorktreeReconciler
 */
function _isWorktreePathStale(_worktreePath: string): boolean {
  // We can't check the filesystem synchronously from the renderer.
  // This is a placeholder that returns false; actual staleness detection
  // should be done in the reconciliation hook (useWorktreeReconciler).
  return false
}

/**
 * Ensure symlinks exist for the active worktree of a project.
 * Reads the project's symlinkDirs setting and creates any missing symlinks.
 * Best-effort: logs warnings on failure but does not block terminal spawn.
 */
export async function ensureWorktreeSymlinks(projectId: string): Promise<void> {
  const project = useProjectStore.getState().projects.find((p) => p.id === projectId)
  if (!project?.path || !project.isGitRepo) return

  const worktree = activeWorktreeOf(project, pickOf(projectId))
  if (!worktree) return

  const symlinkDirs = project.symlinkDirs
  if (!symlinkDirs || symlinkDirs.length === 0) return

  try {
    const result = await worktreeApi.ensureSymlinks(project.path, worktree.path, symlinkDirs)
    if (!result.success) {
      console.warn('[WorktreeSymlinks] Ensure symlinks failed:', result.error)
    }
  } catch (err) {
    console.warn('[WorktreeSymlinks] Ensure symlinks error:', err)
  }
}
