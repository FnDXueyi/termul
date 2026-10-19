import { toast } from 'sonner'
import { filesystemApi } from '@/lib/api'
import { NEW_CANVAS_DOC_NAME, pickCanvasDoc } from '@/lib/canvas-doc'
import { useCanvasStore } from '@/stores/canvas-store'

/**
 * Opens the project's first `.figcraft` document (project root, via the pure
 * `pickCanvasDoc` helper) as the singleton canvas tab. A project without one
 * opens `design.figcraft`, which the editor starts empty and the first save
 * creates. Failure paths toast. Shared by the command palette and the revamp
 * New tab menu.
 */
export async function openProjectCanvas(projectId: string, projectPath: string): Promise<void> {
  const result = await filesystemApi.readDirectory(projectPath)
  if (!result.success) {
    toast.error(`Could not read the project folder: ${result.error}`)
    return
  }
  // Joined from the project path, not the listing's entry path: the desktop
  // listing collapses the `\\?\` of a verbatim root into an invalid `/?/`.
  const name = pickCanvasDoc(result.data)?.name ?? NEW_CANVAS_DOC_NAME
  const docPath = `${projectPath.replace(/[/]+$/, '')}/${name}`
  await useCanvasStore.getState().openCanvas(projectId, docPath)
}
