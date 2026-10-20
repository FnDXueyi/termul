import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  loadSkillFileIndex,
  resetSkillFileIndexCache,
  useSkillFileIndex
} from './use-skill-file-index'

const { listSkillFilesRef, logRef } = vi.hoisted(() => ({
  listSkillFilesRef: { current: vi.fn() },
  logRef: { current: vi.fn() }
}))

vi.mock('@/lib/skills-api', () => ({
  skillsApi: { listSkillFiles: (root?: string) => listSkillFilesRef.current(root) }
}))

vi.mock('@/lib/log-api', () => ({
  logFrontendError: (payload: unknown) => logRef.current(payload)
}))

describe('useSkillFileIndex', () => {
  beforeEach(() => {
    resetSkillFileIndexCache()
    listSkillFilesRef.current = vi.fn(async () => [
      { name: 'Forge-Idea', path: '/p/.claude/skills/forge-idea/SKILL.md' },
      { name: 'forge-idea', path: '/home/u/.agents/skills/forge-idea/SKILL.md' }
    ])
    logRef.current = vi.fn()
  })

  it('keys by lowercased name and keeps the first (highest-priority) entry', async () => {
    const { result } = renderHook(() => useSkillFileIndex('/p'))
    await waitFor(() => expect(result.current.size).toBe(1))
    expect(result.current.get('forge-idea')).toBe('/p/.claude/skills/forge-idea/SKILL.md')
  })

  it('shares one fetch per project root while the index is fresh', async () => {
    await loadSkillFileIndex('/p')
    await loadSkillFileIndex('/p')
    await loadSkillFileIndex('/other')
    expect(listSkillFilesRef.current).toHaveBeenCalledTimes(2)
  })

  it('degrades to an empty index, logs, and refetches after a failure', async () => {
    listSkillFilesRef.current = vi.fn(async () => {
      throw new Error('boom')
    })
    const index = await loadSkillFileIndex('/p')
    expect(index.size).toBe(0)
    expect(logRef.current).toHaveBeenCalledWith(
      expect.objectContaining({ level: 'warn', source: 'useSkillFileIndex' })
    )

    await loadSkillFileIndex('/p')
    expect(listSkillFilesRef.current).toHaveBeenCalledTimes(2)
  })

  it('returns an empty index for a new root until its load resolves', async () => {
    const { result, rerender } = renderHook(({ root }) => useSkillFileIndex(root), {
      initialProps: { root: '/p' }
    })
    await waitFor(() => expect(result.current.size).toBe(1))
    listSkillFilesRef.current = vi.fn(() => new Promise(() => {}))
    rerender({ root: '/other' })
    expect(result.current.size).toBe(0)
  })
})
