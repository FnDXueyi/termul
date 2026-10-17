import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { mockApi } = vi.hoisted(() => ({
  mockApi: {
    renameFile: vi.fn(),
    copyFile: vi.fn(),
    writeFile: vi.fn(),
    getFileInfo: vi.fn(),
    readDirectory: vi.fn(),
    createDirectory: vi.fn()
  }
}))

vi.mock('@/lib/api', () => ({ filesystemApi: mockApi }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { cancelAllAutoSaves } from '@/lib/editor-auto-save'
import { type EditorFileState, useEditorStore } from '@/stores/editor-store'
import { editorTabId, useWorkspaceStore } from '@/stores/workspace-store'
import { getAllLeafPanes } from '@/stores/workspace-store/tree'
import { useFileExplorerStore } from './file-explorer-store'

function openFile(filePath: string, content: string, originalContent: string): void {
  const file: EditorFileState = {
    filePath,
    content,
    originalContent,
    isDirty: content !== originalContent,
    language: 'typescript',
    lastModified: 1,
    viewMode: 'code',
    cursorPosition: { line: 3, col: 7 },
    scrollTop: 40,
    operationStatus: 'idle'
  }
  const files = new Map(useEditorStore.getState().openFiles)
  files.set(filePath, file)
  useEditorStore.setState({ openFiles: files, activeFilePath: filePath })
  useWorkspaceStore.getState().addEditorTab(filePath)
}

function editorTabIds(): string[] {
  return getAllLeafPanes(useWorkspaceStore.getState().root)
    .flatMap((leaf) => leaf.tabs)
    .filter((tab) => tab.type === 'editor')
    .map((tab) => tab.id)
}

async function cutAndPaste(paths: string[], destination: string): Promise<void> {
  useFileExplorerStore.setState({ clipboard: { type: 'cut', paths } })
  await useFileExplorerStore.getState().paste(destination)
}

describe('moving an open file with cut and paste', () => {
  beforeEach(() => {
    mockApi.renameFile.mockReset().mockResolvedValue({ success: true, data: undefined })
    mockApi.copyFile.mockReset().mockResolvedValue({ success: true, data: undefined })
    mockApi.writeFile.mockReset().mockResolvedValue({ success: true, data: undefined })
    mockApi.getFileInfo
      .mockReset()
      .mockResolvedValue({ success: true, data: { type: 'directory' } })
    mockApi.readDirectory.mockReset().mockResolvedValue({ success: true, data: [] })
    useFileExplorerStore.setState({ clipboard: null })
    useEditorStore.setState({ openFiles: new Map(), activeFilePath: null })
    useWorkspaceStore.setState({
      root: { type: 'leaf', id: 'pane-root', tabs: [], activeTabId: null },
      activePaneId: 'pane-root'
    })
  })

  afterEach(() => {
    cancelAllAutoSaves()
  })

  it('moves the tab and the unsaved buffer to the new path and saves there', async () => {
    openFile('/project/src/a.ts', 'unsaved edit', 'on disk')

    await cutAndPaste(['/project/src/a.ts'], '/project/lib')

    expect(mockApi.renameFile).toHaveBeenCalledWith('/project/src/a.ts', '/project/lib/a.ts')
    const { openFiles, activeFilePath } = useEditorStore.getState()
    expect([...openFiles.keys()]).toEqual(['/project/lib/a.ts'])
    expect(activeFilePath).toBe('/project/lib/a.ts')
    expect(openFiles.get('/project/lib/a.ts')).toMatchObject({
      content: 'unsaved edit',
      originalContent: 'on disk',
      isDirty: true
    })
    expect(editorTabIds()).toEqual([editorTabId('/project/lib/a.ts')])

    await expect(useEditorStore.getState().saveFile('/project/lib/a.ts')).resolves.toBe(true)
    expect(mockApi.writeFile).toHaveBeenCalledTimes(1)
    expect(mockApi.writeFile).toHaveBeenCalledWith('/project/lib/a.ts', 'unsaved edit')
  })

  it('moves every open file under a moved folder', async () => {
    openFile('/project/src/a.ts', 'edited a', 'disk a')
    openFile('/project/src/deep/c.ts', 'c', 'c')
    openFile('/project/other/d.ts', 'd', 'd')

    await cutAndPaste(['/project/src'], '/project/lib')

    expect(mockApi.renameFile).toHaveBeenCalledWith('/project/src', '/project/lib/src')
    expect([...useEditorStore.getState().openFiles.keys()]).toEqual([
      '/project/lib/src/a.ts',
      '/project/lib/src/deep/c.ts',
      '/project/other/d.ts'
    ])
    expect(useEditorStore.getState().openFiles.get('/project/lib/src/a.ts')).toMatchObject({
      content: 'edited a',
      isDirty: true
    })
    expect(editorTabIds()).toEqual([
      editorTabId('/project/lib/src/a.ts'),
      editorTabId('/project/lib/src/deep/c.ts'),
      editorTabId('/project/other/d.ts')
    ])
  })

  it('leaves the open file alone when the move fails', async () => {
    mockApi.renameFile.mockResolvedValue({ success: false, error: 'exists' })
    vi.spyOn(console, 'error').mockImplementation(() => {})
    openFile('/project/src/a.ts', 'unsaved edit', 'on disk')

    await cutAndPaste(['/project/src/a.ts'], '/project/lib')

    expect([...useEditorStore.getState().openFiles.keys()]).toEqual(['/project/src/a.ts'])
    expect(editorTabIds()).toEqual([editorTabId('/project/src/a.ts')])
  })

  it('keeps the open file where it is when copying', async () => {
    openFile('/project/src/a.ts', 'unsaved edit', 'on disk')
    useFileExplorerStore.setState({ clipboard: { type: 'copy', paths: ['/project/src/a.ts'] } })

    await useFileExplorerStore.getState().paste('/project/lib')

    expect(mockApi.copyFile).toHaveBeenCalledWith('/project/src/a.ts', '/project/lib/a.ts')
    expect([...useEditorStore.getState().openFiles.keys()]).toEqual(['/project/src/a.ts'])
  })
})
