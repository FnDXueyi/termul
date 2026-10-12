import { beforeEach, describe, expect, it, vi } from 'vitest'

const { mockRead, mockReadFile, mockFetchManifest } = vi.hoisted(() => ({
  mockRead: vi.fn(),
  mockReadFile: vi.fn(),
  mockFetchManifest: vi.fn()
}))

vi.mock('@/lib/api', () => ({
  persistenceApi: { read: mockRead },
  filesystemApi: { readFile: mockReadFile }
}))
vi.mock('@/hooks/use-workspace-manifest-sync', () => ({
  fetchManifestWorkspace: mockFetchManifest
}))
vi.mock('@/hooks/useTerminalAutoSave', () => ({ loadPersistedTerminals: vi.fn(async () => null) }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { useEditorStore } from '@/stores/editor-store'
import { hydrateProject } from './hydrate'

function persistedFile(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    filePath: '/p/a.ts',
    cursorPosition: { line: 4, col: 2 },
    scrollTop: 30,
    viewMode: 'code',
    isDirty: true,
    draftContent: 'draft text',
    lastModified: 100,
    ...overrides
  }
}

function persistedState(file: Record<string, unknown>): { success: true; data: unknown } {
  return {
    success: true,
    data: { openFiles: [file], activeFilePath: '/p/a.ts', expandedDirs: [], activeTabId: null }
  }
}

describe('hydrateProject', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockFetchManifest.mockResolvedValue(null)
  })

  it('stages files off-store: a draft not older than disk is applied and view state restored', async () => {
    mockRead.mockResolvedValue(persistedState(persistedFile()))
    mockReadFile.mockResolvedValue({ success: true, data: { content: 'disk', modifiedAt: 100 } })
    const openFilesBefore = useEditorStore.getState().openFiles

    const staged = await hydrateProject('p1', () => true)

    const file = staged.editorFiles.get('/p/a.ts')
    expect(file).toMatchObject({
      content: 'draft text',
      originalContent: 'disk',
      isDirty: true,
      cursorPosition: { line: 4, col: 2 },
      scrollTop: 30
    })
    expect(staged.activeFilePath).toBe('/p/a.ts')
    expect(staged.tree).not.toBeNull()
    // Nothing was written to the editor store.
    expect(useEditorStore.getState().openFiles).toBe(openFilesBefore)
  })

  it('keeps the disk content when the file changed after the draft was taken', async () => {
    mockRead.mockResolvedValue(persistedState(persistedFile()))
    mockReadFile.mockResolvedValue({ success: true, data: { content: 'disk', modifiedAt: 101 } })

    const staged = await hydrateProject('p1', () => true)

    expect(staged.editorFiles.get('/p/a.ts')).toMatchObject({ content: 'disk', isDirty: false })
  })

  it('skips files that can no longer be read', async () => {
    mockRead.mockResolvedValue(persistedState(persistedFile()))
    mockReadFile.mockResolvedValue({ success: false, error: 'gone' })

    const staged = await hydrateProject('p1', () => true)

    expect(staged.editorFiles.size).toBe(0)
  })

  it('withTree:false stages editor state only and leaves the manifest alone', async () => {
    mockRead.mockResolvedValue(persistedState(persistedFile()))
    mockReadFile.mockResolvedValue({ success: true, data: { content: 'disk', modifiedAt: 1 } })

    const staged = await hydrateProject('p1', () => true, { withTree: false })

    expect(staged.tree).toBeNull()
    expect(staged.manifestRevision).toBeUndefined()
    expect(staged.editorFiles.size).toBe(1)
    expect(mockFetchManifest).not.toHaveBeenCalled()
  })

  it('stops reading once it is no longer current', async () => {
    mockRead.mockResolvedValue(persistedState(persistedFile()))

    await hydrateProject('p1', () => false)

    expect(mockReadFile).not.toHaveBeenCalled()
    expect(mockFetchManifest).not.toHaveBeenCalled()
  })
})
