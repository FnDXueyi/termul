/**
 * Worktree keyboard shortcut handlers.
 *
 * Registers worktree shortcuts from the keyboard-shortcuts store
 * and connects them to project actions. This hook should be used
 * at the workspace level where project context is available.
 *
 * Supported shortcuts:
 * - worktreeCreate: Open NewWorktreeModal
 * - worktreeSwitchNext: Cycle to next worktree
 * - worktreeSwitchPrev: Cycle to previous worktree
 * - worktreeSwitchRoot: Switch to project root
 * - worktreeMergeToMain: Open merge preview
 * - worktreeArchive: Archive active worktree
 * - worktreeOpenTerminal: Dispatch `OPEN_TERMINAL`; `useTabOpeners` handles it by opening a
 *   terminal at the active working copy (it never writes the switcher pick)
 * - worktreeSyncMain: Sync main into worktree (future)
 */

import { useEffect } from 'react'
import { activeWorktreeOf } from '@/lib/active-worktree'
import { normalizePath } from '@/lib/working-copy'
import { getActiveWorktreeForProject } from '@/lib/worktree-context'
import { matchesShortcut, useKeyboardShortcutsStore } from '@/stores/keyboard-shortcuts-store'
import { useProjectStore } from '@/stores/project-store'
import { useShellLayoutStore } from '@/stores/shell-layout-store'
import type { Project, Worktree } from '@/types/project'

// Event name constants for cross-component communication
export const WORKTREE_EVENTS = {
  OPEN_CREATE_MODAL: 'worktree:open-create-modal',
  OPEN_MERGE_DIALOG: 'worktree:open-merge-dialog',
  OPEN_ARCHIVE_DIALOG: 'worktree:open-archive-dialog',
  OPEN_TERMINAL: 'worktree:open-terminal'
} as const

/**
 * Dispatch a custom event to trigger worktree UI actions from shortcuts.
 * This decouples the shortcut handler from specific component state.
 */
export function dispatchWorktreeEvent(event: string, detail?: unknown): void {
  window.dispatchEvent(new CustomEvent(event, { detail }))
}

/**
 * Get the active key for a shortcut ID from the store.
 */
function useGetActiveKey(): (id: string) => string {
  const shortcuts = useKeyboardShortcutsStore((state) => state.shortcuts)
  return (id: string) => {
    const shortcut = shortcuts[id]
    return shortcut?.customKey ?? shortcut?.defaultKey ?? ''
  }
}

/** Listed worktrees ordered by canonical path, `outsideProjectRoot` ones skipped. */
function cycleOrder(project: Project): Worktree[] {
  return (project.worktrees ?? [])
    .filter((w) => w.outsideProjectRoot !== true)
    .map((w) => ({ w, key: normalizePath(w.path) }))
    .sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0))
    .map(({ w }) => w)
}

/**
 * Cycle the pick by `step` (+1 next, -1 prev) over the ordered worktrees,
 * wrapping among worktrees only. From the project folder (no/stale/disabled
 * pick) next goes to the first and prev to the last. The current pick is read
 * from the shell-layout store at event time.
 */
function cyclePick(project: Project, step: 1 | -1): void {
  const order = cycleOrder(project)
  if (order.length === 0) return
  const state = useShellLayoutStore.getState()
  const current = activeWorktreeOf(project, state.worktreePickByProject[project.id])
  const idx = current ? order.findIndex((w) => w.id === current.id) : -1
  let nextIdx: number
  if (idx === -1) nextIdx = step === 1 ? 0 : order.length - 1
  else nextIdx = (idx + step + order.length) % order.length
  state.setWorktreePick(project.id, order[nextIdx].path)
}

export function useWorktreeShortcuts(): void {
  const getActiveKey = useGetActiveKey()
  const projects = useProjectStore((state) => state.projects)
  const activeProjectId = useProjectStore((state) => state.activeProjectId)

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent): void => {
      if (e.defaultPrevented) return
      if (!activeProjectId) return

      const project = projects.find((p) => p.id === activeProjectId)
      if (!project) return

      // Worktree Create — open modal via custom event
      if (matchesShortcut(e, getActiveKey('worktreeCreate'))) {
        e.preventDefault()
        e.stopPropagation()
        dispatchWorktreeEvent(WORKTREE_EVENTS.OPEN_CREATE_MODAL, { projectId: activeProjectId })
        return
      }

      // Switch to next worktree
      if (matchesShortcut(e, getActiveKey('worktreeSwitchNext'))) {
        e.preventDefault()
        cyclePick(project, 1)
        return
      }

      // Switch to previous worktree
      if (matchesShortcut(e, getActiveKey('worktreeSwitchPrev'))) {
        e.preventDefault()
        cyclePick(project, -1)
        return
      }

      // Switch to project root
      if (matchesShortcut(e, getActiveKey('worktreeSwitchRoot'))) {
        e.preventDefault()
        useShellLayoutStore.getState().setWorktreePick(project.id, null)
        return
      }

      // Merge to main — open merge dialog via custom event
      if (matchesShortcut(e, getActiveKey('worktreeMergeToMain'))) {
        e.preventDefault()
        e.stopPropagation()
        const activeWt = getActiveWorktreeForProject(activeProjectId)
        if (activeWt) {
          dispatchWorktreeEvent(WORKTREE_EVENTS.OPEN_MERGE_DIALOG, {
            worktreePath: activeWt.path,
            projectPath: project.path,
            sourceBranch: activeWt.branch,
            projectName: project.name,
            targetBranch: 'main'
          })
        }
        return
      }

      // Archive active worktree — open archive/removal dialog via custom event
      if (matchesShortcut(e, getActiveKey('worktreeArchive'))) {
        e.preventDefault()
        e.stopPropagation()
        const activeWt = getActiveWorktreeForProject(activeProjectId)
        if (activeWt) {
          dispatchWorktreeEvent(WORKTREE_EVENTS.OPEN_ARCHIVE_DIALOG, {
            worktree: activeWt,
            projectId: activeProjectId
          })
        }
        return
      }

      // Open a terminal at the active working copy (handled by useTabOpeners)
      if (matchesShortcut(e, getActiveKey('worktreeOpenTerminal'))) {
        e.preventDefault()
        dispatchWorktreeEvent(WORKTREE_EVENTS.OPEN_TERMINAL, { projectId: activeProjectId })
        return
      }

      // Sync main into worktree (future)
      if (matchesShortcut(e, getActiveKey('worktreeSyncMain'))) {
        e.preventDefault()
        const activeWt = getActiveWorktreeForProject(activeProjectId)
        if (activeWt) {
          dispatchWorktreeEvent(WORKTREE_EVENTS.OPEN_MERGE_DIALOG, {
            worktreePath: activeWt.path,
            projectPath: project.path,
            sourceBranch: 'main',
            projectName: project.name,
            targetBranch: activeWt.branch
          })
        }
        return
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [activeProjectId, projects, getActiveKey])
}
