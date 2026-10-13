/**
 * Off-store hydration for the project-switch coordinator: replicates the read
 * path of the old in-hook restore (persisted editor state, host manifest,
 * open files, legacy pane layout) but writes NOTHING to any store. The result
 * is a {@link StagedProject} the coordinator commits atomically once it is
 * still the latest request; `isCurrent()` is checked after every await so a
 * superseded hydration stops early (its return value is then discarded).
 */

import type { FetchedManifestWorkspace } from '@/hooks/use-workspace-manifest-sync'
import { fetchManifestWorkspace } from '@/hooks/use-workspace-manifest-sync'
import { loadPersistedTerminals } from '@/hooks/useTerminalAutoSave'
import { filesystemApi, persistenceApi } from '@/lib/api'
import { emptyStagedProject, type StagedProject } from '@/lib/project-switch/types'
import type { EditorFileState } from '@/stores/editor-store'
import { detectLanguage, getDefaultViewMode, reconcileDraft } from '@/stores/editor-store'
import { useProjectStore } from '@/stores/project-store'
import { useTerminalStore } from '@/stores/terminal-store'
import {
  buildProjectWorkspace,
  createProjectWorkspace,
  editorTabId,
  type ProjectWorkspace
} from '@/stores/workspace-store'
import type { LeafNode, PaneNode } from '@/types/workspace.types'
import { mergeNonPortableTabs } from './manifest-merge'
import { deserializePaneTree, reconcileTerminalTabs } from './pane-tree'
import {
  editorStateKey,
  filterExpandedDirsByRoot,
  type PersistedEditorFile,
  type PersistedEditorState
} from './types'

export type { StagedProject } from '@/lib/project-switch/types'

/** Read one file into a staged editor state; a deleted/unreadable file yields null. */
async function stageFile(filePath: string): Promise<EditorFileState | null> {
  try {
    const result = await filesystemApi.readFile(filePath)
    if (!result.success) return null
    return {
      filePath,
      content: result.data.content,
      originalContent: result.data.content,
      isDirty: false,
      language: detectLanguage(filePath),
      lastModified: result.data.modifiedAt,
      viewMode: getDefaultViewMode(filePath),
      cursorPosition: { line: 1, col: 1 },
      scrollTop: 0,
      operationStatus: 'idle'
    }
  } catch {
    // File may have been deleted since last session.
    return null
  }
}

/**
 * Apply a persisted unsaved draft to the freshly read file: the draft wins when
 * the file on disk is not newer than the draft; a newer file shows a conflict.
 */
function applyPersistedDraft(
  file: EditorFileState,
  persisted: PersistedEditorFile
): EditorFileState {
  if (!persisted.isDirty || !persisted.draftContent) return file
  return reconcileDraft(file, { content: persisted.draftContent, savedAt: persisted.lastModified })
}

function applyPersistedViewState(
  file: EditorFileState,
  persisted: PersistedEditorFile
): EditorFileState {
  return applyPersistedDraft(
    {
      ...file,
      cursorPosition: { line: persisted.cursorPosition.line, col: persisted.cursorPosition.col },
      scrollTop: persisted.scrollTop,
      viewMode: persisted.viewMode !== 'code' ? persisted.viewMode : file.viewMode
    },
    persisted
  )
}

/** Fresh single-leaf layout carrying the open files as tabs (legacy, pre-paneLayout state). */
function legacyEditorWorkspace(
  filePaths: string[],
  restoredActiveTabId: string | null
): ProjectWorkspace {
  const fresh = createProjectWorkspace()
  const tabs = [...new Set(filePaths)].map((filePath) => ({
    type: 'editor' as const,
    id: editorTabId(filePath),
    filePath
  }))
  let activeTabId = restoredActiveTabId
  if (!activeTabId || !tabs.some((tab) => tab.id === activeTabId)) {
    activeTabId = tabs.length > 0 ? tabs[tabs.length - 1].id : null
  }
  const leaf: LeafNode = { ...(fresh.root as LeafNode), tabs, activeTabId }
  return { ...fresh, root: leaf }
}

async function stageManifestEditors(
  fetched: FetchedManifestWorkspace,
  editorFiles: Map<string, EditorFileState>
): Promise<void> {
  const missing = fetched.editors.filter((descriptor) => !editorFiles.has(descriptor.filePath))
  const staged = await Promise.all(missing.map((descriptor) => stageFile(descriptor.filePath)))
  staged.forEach((file, index) => {
    if (file) editorFiles.set(missing[index].filePath, file)
  })
}

/**
 * Build everything needed to show `projectId`. `withTree: false` is for a
 * project that already has a parked tree: only its editor state is staged
 * (`tree: null`, manifest untouched).
 */
export async function hydrateProject(
  projectId: string,
  isCurrent: () => boolean,
  options: { withTree?: boolean } = {}
): Promise<StagedProject> {
  const withTree = options.withTree !== false
  const abandoned = emptyStagedProject()

  const result = await persistenceApi.read<PersistedEditorState>(editorStateKey(projectId))
  if (!isCurrent()) return abandoned

  if (!result.success || !result.data) {
    if (!withTree) return { ...emptyStagedProject(), tree: null, manifestRevision: undefined }
    // No renderer-local editor state. A host manifest may still exist
    // (cross-client: this client never opened the project before).
    const fetched = await fetchManifestWorkspace(projectId)
    if (!isCurrent()) return abandoned
    const editorFiles = new Map<string, EditorFileState>()
    if (fetched) {
      await stageManifestEditors(fetched, editorFiles)
      if (!isCurrent()) return abandoned
    }
    return {
      tree: fetched
        ? buildProjectWorkspace(fetched.root, fetched.activePaneId)
        : createProjectWorkspace(),
      editorFiles,
      activeFilePath: null,
      expandedDirs: null,
      focusedSessionId: fetched?.focusedSessionId ?? null,
      manifestRevision: fetched?.revision ?? null,
      persistedLayout: undefined
    }
  }

  const persisted = result.data
  const rootPath = useProjectStore
    .getState()
    .projects.find((project) => project.id === projectId)?.path
  const expandedDirs = filterExpandedDirsByRoot(persisted.expandedDirs, rootPath)

  const read = await Promise.all(persisted.openFiles.map((file) => stageFile(file.filePath)))
  if (!isCurrent()) return abandoned
  const editorFiles = new Map<string, EditorFileState>()
  persisted.openFiles.forEach((file, index) => {
    const staged = read[index]
    if (staged) editorFiles.set(file.filePath, applyPersistedViewState(staged, file))
  })

  if (!withTree) {
    return {
      tree: null,
      editorFiles,
      activeFilePath: persisted.activeFilePath,
      expandedDirs,
      focusedSessionId: null,
      manifestRevision: undefined,
      persistedLayout: persisted.paneLayout
    }
  }

  // The manifest is authoritative for portable topology (terminalIds +
  // editorIds + activeTabId + activePaneId); else fall back to the legacy
  // editorStateKey.paneLayout (carrying every tab variant incl.
  // browser/git/agent-chat). Non-portable variants are absent on a manifest
  // restore and are merged back over its topology below.
  const fetched = await fetchManifestWorkspace(projectId)
  if (!isCurrent()) return abandoned
  if (fetched) {
    await stageManifestEditors(fetched, editorFiles)
    if (!isCurrent()) return abandoned
  }

  let tree: ProjectWorkspace
  if (fetched) {
    let root: PaneNode = fetched.root
    if (persisted.paneLayout) {
      root = mergeNonPortableTabs(root, fetched.activePaneId, persisted.paneLayout, projectId).root
    }
    tree = buildProjectWorkspace(root, fetched.activePaneId)
  } else if (persisted.paneLayout) {
    const liveProjectTerminals = useTerminalStore
      .getState()
      .terminals.filter((terminal) => terminal.projectId === projectId && !!terminal.ptyId)
    const persistedTerminalLayout = await loadPersistedTerminals(projectId)
    if (!isCurrent()) return abandoned

    // Deserialize AFTER the await, right before the commit: the session index
    // may land during it, and its prune only scans the committed tree —
    // judging ownership earlier would let a now-known foreign chat slip in.
    // Unknown ownership stays fail-open.
    const restoredTree = deserializePaneTree(persisted.paneLayout, projectId)
    const cleanTree = reconcileTerminalTabs(
      restoredTree,
      new Set(editorFiles.keys()),
      liveProjectTerminals,
      persistedTerminalLayout
    )
    tree = buildProjectWorkspace(cleanTree, persisted.activePaneId)
  } else {
    tree = legacyEditorWorkspace([...editorFiles.keys()], persisted.activeTabId)
  }

  return {
    tree,
    editorFiles,
    activeFilePath: persisted.activeFilePath,
    expandedDirs,
    focusedSessionId: fetched?.focusedSessionId ?? null,
    manifestRevision: fetched?.revision ?? null,
    persistedLayout: persisted.paneLayout
  }
}
