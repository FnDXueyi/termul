import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { logFrontendError } from '@/lib/log-api'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { getAllLeafPanes, useWorkspaceStore } from '@/stores/workspace-store'
import type { PaneNode } from '@/types/workspace.types'
import { mergeNonPortableTabs } from './manifest-merge'
import type { PersistedPaneNodeInput } from './types'

function leafTabIds(paneId: string): string[] {
  const leaf = getAllLeafPanes(useWorkspaceStore.getState().root).find((l) => l.id === paneId)
  return leaf?.tabs.map((tab) => tab.id) ?? []
}

function leafActive(paneId: string): string | null | undefined {
  return getAllLeafPanes(useWorkspaceStore.getState().root).find((l) => l.id === paneId)
    ?.activeTabId
}

function seed(root: PaneNode, activePaneId: string): void {
  useWorkspaceStore.getState().loadProjectWorkspace(root, activePaneId)
}

const manifestTree: PaneNode = {
  type: 'split',
  id: 'split-1',
  direction: 'horizontal',
  sizes: [50, 50],
  children: [
    {
      type: 'leaf',
      id: 'A',
      tabs: [{ type: 'terminal', id: 'term-t1', terminalId: 't1' }],
      activeTabId: 'term-t1'
    },
    {
      type: 'leaf',
      id: 'B',
      tabs: [{ type: 'editor', id: 'editor:/a.ts', filePath: '/a.ts' }],
      activeTabId: 'editor:/a.ts'
    }
  ]
}

describe('mergeNonPortableTabs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useBrowserSessionStore.setState({ tabs: new Map() })
    seed(manifestTree, 'B')
  })

  it('appends non-portable tabs to the leaf with the same id and keeps manifest state', () => {
    const layout: PersistedPaneNodeInput = {
      type: 'leaf',
      id: 'A',
      activeTabId: 'git-/repo',
      tabs: [
        { type: 'terminal', terminalId: 't1' },
        { type: 'browser', browserTabId: 'b1', url: 'https://example.com' },
        { type: 'git', id: 'git-/repo', cwd: '/repo' },
        { type: 'git-history', id: 'git-history-/repo', cwd: '/repo' },
        { type: 'agent-chat', id: 'chat-s1', sessionId: 's1' }
      ]
    }

    expect(mergeNonPortableTabs(layout, 'p1')).toBe(3)

    const ids = leafTabIds('A')
    expect(ids[0]).toBe('term-t1')
    expect(ids).toHaveLength(4)
    expect(ids).toContain('git-/repo')
    expect(ids).toContain('git-history-/repo')
    expect(ids.some((id) => id.includes('b1'))).toBe(true)
    expect(ids).not.toContain('chat-s1')
    expect(leafTabIds('B')).toEqual(['editor:/a.ts'])
    expect(leafActive('A')).toBe('term-t1')
    expect(useWorkspaceStore.getState().activePaneId).toBe('B')
    expect(useBrowserSessionStore.getState().tabs.get('b1')?.url).toBe('https://example.com')
    expect(logFrontendError).toHaveBeenCalledTimes(1)
  })

  it('falls back to the active pane when no leaf has the persisted id', () => {
    const layout: PersistedPaneNodeInput = {
      type: 'leaf',
      id: 'other-client-leaf',
      activeTabId: null,
      tabs: [{ type: 'git', id: 'git-/repo', cwd: '/repo' }]
    }

    expect(mergeNonPortableTabs(layout, 'p1')).toBe(1)

    expect(leafTabIds('B')).toEqual(['editor:/a.ts', 'git-/repo'])
    expect(leafTabIds('A')).toEqual(['term-t1'])
    expect(leafActive('B')).toBe('editor:/a.ts')
  })

  it('merges each persisted leaf of a split layout into its matching pane', () => {
    const layout: PersistedPaneNodeInput = {
      type: 'split',
      id: 'split-1',
      direction: 'horizontal',
      sizes: [50, 50],
      children: [
        {
          type: 'leaf',
          id: 'A',
          activeTabId: null,
          tabs: [{ type: 'git', id: 'git-/a', cwd: '/a' }]
        },
        {
          type: 'leaf',
          id: 'B',
          activeTabId: null,
          tabs: [{ type: 'git-history', id: 'git-history-/b', cwd: '/b' }]
        }
      ]
    }

    expect(mergeNonPortableTabs(layout, 'p1')).toBe(2)

    expect(leafTabIds('A')).toEqual(['term-t1', 'git-/a'])
    expect(leafTabIds('B')).toEqual(['editor:/a.ts', 'git-history-/b'])
  })

  it('adopts the persisted active tab only when the leaf had none', () => {
    seed({ type: 'leaf', id: 'A', tabs: [], activeTabId: null }, 'A')
    const layout: PersistedPaneNodeInput = {
      type: 'leaf',
      id: 'A',
      activeTabId: 'git-/b',
      tabs: [
        { type: 'git', id: 'git-/a', cwd: '/a' },
        { type: 'git', id: 'git-/b', cwd: '/b' }
      ]
    }

    mergeNonPortableTabs(layout, 'p1')

    expect(leafTabIds('A')).toEqual(['git-/a', 'git-/b'])
    expect(leafActive('A')).toBe('git-/b')
  })

  it('does not duplicate ids already in the live tree and skips the commit when nothing is new', () => {
    seed(
      {
        type: 'leaf',
        id: 'A',
        tabs: [{ type: 'git', id: 'git-/repo', cwd: '/repo' }],
        activeTabId: 'git-/repo'
      },
      'A'
    )
    const layout: PersistedPaneNodeInput = {
      type: 'leaf',
      id: 'Z',
      activeTabId: null,
      tabs: [
        { type: 'git', id: 'git-/repo', cwd: '/repo' },
        { type: 'browser', browserTabId: 'b1' }
      ]
    }

    expect(mergeNonPortableTabs(layout, 'p1')).toBe(1)
    expect(leafTabIds('A').filter((id) => id === 'git-/repo')).toHaveLength(1)

    const rootAfterFirst = useWorkspaceStore.getState().root
    expect(mergeNonPortableTabs(layout, 'p1')).toBe(0)
    expect(useWorkspaceStore.getState().root).toBe(rootAfterFirst)
  })

  it('merges nothing for a missing layout, a legacy leaf or portable-only tabs', () => {
    const rootBefore = useWorkspaceStore.getState().root
    expect(mergeNonPortableTabs(undefined, 'p1')).toBe(0)
    expect(
      mergeNonPortableTabs(
        { type: 'leaf', id: 'A', editorFilePaths: ['/a.ts'], activeTabId: null },
        'p1'
      )
    ).toBe(0)
    expect(
      mergeNonPortableTabs(
        {
          type: 'leaf',
          id: 'A',
          activeTabId: null,
          tabs: [{ type: 'terminal', terminalId: 't9' }]
        },
        'p1'
      )
    ).toBe(0)
    expect(useWorkspaceStore.getState().root).toBe(rootBefore)
    expect(logFrontendError).not.toHaveBeenCalled()
  })

  it('ignores unknown or corrupt refs', () => {
    const layout = {
      type: 'leaf',
      id: 'A',
      activeTabId: null,
      tabs: [null, { type: 'hologram', id: 'x' }, { type: 'canvas', id: 'c' }]
    } as unknown as PersistedPaneNodeInput

    expect(mergeNonPortableTabs(layout, 'p1')).toBe(0)
    expect(leafTabIds('A')).toEqual(['term-t1'])
  })
})
