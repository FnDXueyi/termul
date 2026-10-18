import type { DirectoryEntry } from '@shared/types/filesystem.types'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { readDirectory, openCanvas } = vi.hoisted(() => ({
  readDirectory: vi.fn(),
  openCanvas: vi.fn()
}))

vi.mock('@/lib/api', () => ({ filesystemApi: { readDirectory } }))
vi.mock('@/stores/canvas-store', () => ({
  useCanvasStore: { getState: () => ({ openCanvas }) }
}))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))

import { openProjectCanvas } from '../open-project-canvas'

const VERBATIM_ROOT = String.raw`\\?\D:\work\proj`

function listing(...entries: Array<Pick<DirectoryEntry, 'name' | 'path'>>): unknown {
  return {
    success: true,
    data: entries.map((entry) => ({
      ...entry,
      type: 'file',
      extension: null,
      size: 0,
      modifiedAt: 0
    }))
  }
}

describe('openProjectCanvas', () => {
  beforeEach(() => {
    readDirectory.mockReset()
    openCanvas.mockReset()
  })

  it('reopens an existing document under the same path the first open used', async () => {
    readDirectory.mockResolvedValueOnce(listing())
    await openProjectCanvas('p1', VERBATIM_ROOT)
    // The desktop listing collapses the leading `//` of a `\\?\` root.
    readDirectory.mockResolvedValueOnce(
      listing({ name: 'design.figcraft', path: '/?/D:/work/proj/design.figcraft' })
    )
    await openProjectCanvas('p1', VERBATIM_ROOT)

    const [first, reopen] = openCanvas.mock.calls.map(([, docPath]) => docPath)
    expect(first).toBe(String.raw`\\?\D:\work\proj/design.figcraft`)
    expect(reopen).toBe(first)
  })

  it('opens the alphabetically-first canvas document by name under the project root', async () => {
    readDirectory.mockResolvedValueOnce(
      listing(
        { name: 'poster.figcraft', path: '/proj/poster.figcraft' },
        { name: 'brand.figcraft', path: '/proj/brand.figcraft' }
      )
    )
    await openProjectCanvas('p1', '/proj/')
    expect(openCanvas).toHaveBeenCalledWith('p1', '/proj/brand.figcraft')
  })
})
