import { flushEditorContent } from '@/lib/editor-content-flush'
import { logFrontendError } from '@/lib/log-api'
import { basename } from '@/lib/path-basename'
import { useEditorStore } from '@/stores/editor-store'
import { useWorkspaceStore } from '@/stores/workspace-store'

function forwardSlashes(path: string): string {
  return path.replace(/\\/g, '/')
}

/**
 * Follow a file or folder rename in the editor: every open file at or under
 * `fromPath` moves to the matching path under `toPath`, keeping its buffer and
 * unsaved edits, and its workspace tab moves with it.
 */
export async function moveOpenEditorTabs(fromPath: string, toPath: string): Promise<void> {
  const from = forwardSlashes(fromPath)
  const to = forwardSlashes(toPath)
  const moves: Array<[string, string]> = []
  for (const openPath of useEditorStore.getState().openFiles.keys()) {
    const open = forwardSlashes(openPath)
    if (open === from || open.startsWith(`${from}/`)) {
      moves.push([openPath, `${to}${open.slice(from.length)}`])
    }
  }

  for (const [oldPath, newPath] of moves) {
    try {
      // Pull any keystrokes the editor has not handed to the store yet.
      await flushEditorContent(oldPath)
    } catch (error) {
      void logFrontendError({
        level: 'warn',
        source: 'editor-tab-rename',
        message: `editor flush before rename failed file=${basename(oldPath)}: ${
          error instanceof Error ? error.message : String(error)
        }`
      })
    }
    useEditorStore.getState().renameOpenFile(oldPath, newPath)
    useWorkspaceStore.getState().renameEditorTab(oldPath, newPath)
  }
}
