import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { LeafNode, SplitNode } from '@/types/workspace.types'
import type { WorkspaceState, WorkspaceTab } from './workspace-store'
import { useWorkspaceStore } from './workspace-store'

vi.mock('@/lib/log-api', () => ({
  logFrontendError: vi.fn().mockResolvedValue(undefined)
}))

function leavesOf(node: WorkspaceState['root']): LeafNode[] {
  if (node.type === 'leaf') return [node]
  return node.children.flatMap(leavesOf)
}

const diffTabs = (): WorkspaceTab[] =>
  leavesOf(useWorkspaceStore.getState().root).flatMap((leaf) =>
    leaf.tabs.filter((t) => t.type === 'diff')
  )

describe('workspace-store addDiffTab', () => {
  beforeEach(() => {
    useWorkspaceStore.setState(() => {
      const root: LeafNode = { type: 'leaf', id: 'pane-root', tabs: [], activeTabId: null }
      return { root, activePaneId: 'pane-root', fullscreenPaneId: null }
    })
  })

  it('collapses opens with different path forms of one identity into one focused tab', () => {
    const store = useWorkspaceStore.getState()
    store.addDiffTab('E:\\p', 'a\\b.ts', false, 'pane-root')
    store.addDiffTab('e:/p/', 'a/b.ts', false, 'pane-root')

    const tabs = diffTabs()
    expect(tabs).toHaveLength(1)
    expect(tabs[0].id).toMatch(/^diff:/)
    expect(tabs[0]).toMatchObject({ cwd: 'E:\\p', path: 'a\\b.ts', staged: false })
    expect((useWorkspaceStore.getState().root as LeafNode).activeTabId).toBe(tabs[0].id)
  })

  it('focuses the existing tab in another pane instead of adding one', () => {
    const store = useWorkspaceStore.getState()
    store.addDiffTab('/repo', 'a.ts', true, 'pane-root')
    store.splitPane(
      'pane-root',
      'horizontal',
      { type: 'editor', id: 'edit-/a.ts', filePath: '/a.ts' },
      'right'
    )
    const split = useWorkspaceStore.getState().root as SplitNode
    const leftPaneId = (split.children[0] as LeafNode).id
    const rightPaneId = (split.children[1] as LeafNode).id
    store.setActivePane(rightPaneId)

    store.addDiffTab('/repo', 'a.ts', true, rightPaneId)

    expect(diffTabs()).toHaveLength(1)
    expect(useWorkspaceStore.getState().activePaneId).toBe(leftPaneId)
    const holder = leavesOf(useWorkspaceStore.getState().root).find((leaf) =>
      leaf.tabs.some((t) => t.type === 'diff')
    )
    expect(holder?.id).toBe(leftPaneId)
    expect(holder?.activeTabId).toBe(diffTabs()[0].id)
  })

  it('keeps distinct tabs per side and per working copy', () => {
    const store = useWorkspaceStore.getState()
    store.addDiffTab('/repo', 'a.ts', false, 'pane-root')
    store.addDiffTab('/repo', 'a.ts', true, 'pane-root')
    store.addDiffTab('/repo-wt', 'a.ts', false, 'pane-root')
    expect(new Set(diffTabs().map((t) => t.id)).size).toBe(3)
  })

  it('adds a new tab to the active pane when no target is given', () => {
    useWorkspaceStore.getState().addDiffTab('/repo', 'a.ts', false)
    expect((useWorkspaceStore.getState().root as LeafNode).tabs).toHaveLength(1)
  })
})
