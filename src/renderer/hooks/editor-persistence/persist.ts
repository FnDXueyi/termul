import { persistenceApi } from '@/lib/api'
import type { EditorFileState } from '@/stores/editor-store'
import { useEditorStore } from '@/stores/editor-store'
import { useFileExplorerStore } from '@/stores/file-explorer-store'
import { findPaneById, useWorkspaceStore } from '@/stores/workspace-store'
import { chatForeignToProject, serializePaneTree } from './pane-tree'
import { editorStateKey, type PersistedEditorFile, type PersistedEditorState } from './types'

/**
 * Persist the VISIBLE project's editor/explorer/pane state under
 * `editor-state/<projectId>`. Refuses for any project whose tree is not the
 * committed one: the editor, explorer and flat pane fields always describe
 * the committed project, so writing them under another id would corrupt that
 * project's persisted state (a pending switch's target, a stale timer).
 */
export function persistState(projectId: string): void {
  const workspaceState = useWorkspaceStore.getState()
  if (workspaceState.activeProjectId !== projectId) return

  const editorState = useEditorStore.getState()
  const explorerState = useFileExplorerStore.getState()

  const openFiles: PersistedEditorFile[] = []
  editorState.openFiles.forEach((file: EditorFileState) => {
    const persisted: PersistedEditorFile = {
      filePath: file.filePath,
      cursorPosition: file.cursorPosition,
      scrollTop: file.scrollTop,
      viewMode: file.viewMode,
      isDirty: file.isDirty,
      lastModified: file.lastModified
    }
    if (file.isDirty) {
      persisted.draftContent = file.content
    }
    openFiles.push(persisted)
  })

  const expandedDirs = Array.from(explorerState.expandedDirs)

  const data: PersistedEditorState = {
    openFiles,
    activeFilePath: editorState.activeFilePath,
    expandedDirs,
    activeTabId: (() => {
      const pane = findPaneById(workspaceState.root, workspaceState.activePaneId)
      return pane && pane.type === 'leaf' ? pane.activeTabId : null
    })(),
    // Project ownership filter (see retainVisibleAgentChats): agent-chat tabs
    // of OTHER projects can be present when this project's state is saved.
    // Persisting them seeds cross-project tab leaks on restore.
    paneLayout: serializePaneTree(workspaceState.root, {
      // Keep when NOT known-foreign (fail-open — see chatForeignToProject).
      agentChatOwnedBy: (sessionId) => !chatForeignToProject(sessionId, projectId)
    }),
    activePaneId: workspaceState.activePaneId
  }

  persistenceApi.writeDebounced(editorStateKey(projectId), data)
}
