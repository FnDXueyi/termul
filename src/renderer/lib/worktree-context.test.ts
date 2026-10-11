import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Project } from '@/types/project'

const { projectStoreMock } = vi.hoisted(() => ({
  projectStoreMock: { getState: vi.fn() }
}))

// worktree-context.ts imports worktreeApi at module load; the mock prevents
// the real API from firing during tests. The tested functions don't call it,
// but the import binding must resolve.
vi.mock('@/lib/api', () => ({
  worktreeApi: {
    list: vi.fn(),
    create: vi.fn(),
    remove: vi.fn(),
    branches: vi.fn(),
    ensureSymlinks: vi.fn(),
    copyIncludeFiles: vi.fn()
  }
}))

vi.mock('@/stores/project-store', () => ({
  useProjectStore: projectStoreMock
}))

import { useShellLayoutStore } from '@/stores/shell-layout-store'
import {
  getActiveWorktreeForProject,
  getDefaultCwdForProject,
  getProjectRootPath,
  getWorktreeContextLabel,
  getWorktreeTabContext
} from './worktree-context'

function makeProject(overrides: Partial<Project> = {}): Project {
  return {
    id: 'p1',
    name: 'Test',
    color: 'blue',
    path: '/projects/main',
    ...overrides
  } as Project
}

const WT_PATH = '/projects/main/.termul/worktrees/abc'
const WORKTREE = {
  id: 'wt-1',
  name: 'chat/abc',
  branch: 'chat/abc',
  path: WT_PATH,
  createdAt: '2026-01-01'
}

function setPick(path: string | null): void {
  useShellLayoutStore.setState({ worktreePickByProject: path === null ? {} : { p1: path } })
}

function setState(projects: Project[]): void {
  projectStoreMock.getState.mockReturnValue({ projects })
}

describe('worktree-context', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setPick(null)
  })

  describe('getDefaultCwdForProject', () => {
    it('returns the main project root path', () => {
      setState([makeProject()])
      expect(getDefaultCwdForProject('p1')).toBe('/projects/main')
    })

    it('returns the project root even when an active worktree is set', () => {
      setState([
        makeProject({
          worktrees: [WORKTREE]
        })
      ])
      setPick(WT_PATH)
      expect(getDefaultCwdForProject('p1')).toBe('/projects/main')
    })

    it('returns the project root when there is no pick', () => {
      setState([makeProject()])
      expect(getDefaultCwdForProject('p1')).toBe('/projects/main')
    })

    it('returns empty string when the project is not found', () => {
      setState([])
      expect(getDefaultCwdForProject('missing')).toBe('')
    })

    it('returns empty string when the project has no path', () => {
      setState([makeProject({ path: '' })])
      expect(getDefaultCwdForProject('p1')).toBe('')
    })

    it('returns empty string when project.path is undefined (optional field)', () => {
      setState([makeProject({ path: undefined })])
      expect(getDefaultCwdForProject('p1')).toBe('')
    })
  })

  describe('getProjectRootPath', () => {
    it('returns the main project root path regardless of active worktree', () => {
      setState([
        makeProject({
          worktrees: [WORKTREE]
        })
      ])
      setPick(WT_PATH)
      expect(getProjectRootPath('p1')).toBe('/projects/main')
    })
  })

  describe('active worktree derived from the shell-layout pick', () => {
    it('sees the picked worktree in any path form', () => {
      const project = makeProject({ worktrees: [WORKTREE] })
      setState([project])
      setPick(`${WT_PATH}/`)
      expect(getActiveWorktreeForProject('p1')).toEqual(WORKTREE)
      expect(getWorktreeTabContext('p1')).toBe('chat/abc')
      expect(getWorktreeContextLabel(project)).toBe('Test / chat/abc')
    })

    it('sees the project folder with no pick', () => {
      const project = makeProject({ worktrees: [WORKTREE] })
      setState([project])
      expect(getActiveWorktreeForProject('p1')).toBeUndefined()
      expect(getWorktreeTabContext('p1')).toBeNull()
      expect(getWorktreeContextLabel(project)).toBe('Test / Root')
    })

    it('sees the project folder when the pick equals the project path', () => {
      setState([makeProject({ worktrees: [WORKTREE] })])
      setPick('/projects/main')
      expect(getActiveWorktreeForProject('p1')).toBeUndefined()
    })

    it('sees the project folder for a pruned pick', () => {
      const project = makeProject({ worktrees: [WORKTREE] })
      setState([project])
      setPick('/projects/main/.termul/worktrees/gone')
      expect(getActiveWorktreeForProject('p1')).toBeUndefined()
      expect(getWorktreeContextLabel(project)).toBe('Test / Root')
    })

    it('sees the project folder for a pick naming an outside-root worktree', () => {
      const project = makeProject({ worktrees: [{ ...WORKTREE, outsideProjectRoot: true }] })
      setState([project])
      setPick(WT_PATH)
      expect(getActiveWorktreeForProject('p1')).toBeUndefined()
    })

    it('sees nothing for a missing project', () => {
      setState([])
      setPick(WT_PATH)
      expect(getActiveWorktreeForProject('p1')).toBeUndefined()
    })
  })
})
