import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { logFrontendError } from '@/lib/log-api'
import { useAgentChatLifetimeStore } from '@/stores/agent-chat-lifetime-store'
import { useProjectStore } from '@/stores/project-store'
import { getAllLeafPanes, useWorkspaceStore } from '@/stores/workspace-store'
import type { Project } from '@/types/project'
import { pruneRestoredChatTabs } from './chat-tab-prune'

function project(id: string, isArchived?: boolean): Project {
  return { id, name: id, color: 'blue', isArchived } as Project
}

function chatTab(sessionId: string): {
  type: 'agent-chat'
  id: string
  sessionId: string
} {
  return { type: 'agent-chat', id: `chat-${sessionId}`, sessionId }
}

function seedTree(sessionIds: string[]): void {
  useWorkspaceStore.getState().loadProjectWorkspace({
    type: 'leaf',
    id: 'pane-1',
    activeTabId: sessionIds[0] ? `chat-${sessionIds[0]}` : null,
    tabs: sessionIds.map(chatTab)
  })
}

function treeSessionIds(): string[] {
  return getAllLeafPanes(useWorkspaceStore.getState().root).flatMap((leaf) =>
    leaf.tabs.flatMap((tab) => (tab.type === 'agent-chat' ? [tab.sessionId] : []))
  )
}

const ownership = (owners: Record<string, string>) => ({
  sessions: {},
  sessionIndex: Object.entries(owners).map(([id, projectId]) => ({ id, projectId }))
})

function prune(owners: Record<string, string>, live: string[] = []): void {
  pruneRestoredChatTabs({
    liveSessionIds: new Set(live),
    indexedSessionIds: new Set(Object.keys(owners)),
    ownership: ownership(owners),
    activeProjectId: 'p1'
  })
}

describe('pruneRestoredChatTabs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useProjectStore.setState({
      activeProjectId: 'p1',
      projects: [project('p1'), project('p2')]
    })
    useAgentChatLifetimeStore.setState({ retainedByProject: {}, focusSessionByProject: {} })
  })

  it('removes a corpse tab that is neither live nor indexed', () => {
    seedTree(['s-corpse', 's-own'])
    prune({ 's-own': 'p1' })
    expect(treeSessionIds()).toEqual(['s-own'])
  })

  it('keeps a live session that the index does not know yet', () => {
    seedTree(['s-live'])
    prune({}, ['s-live'])
    expect(treeSessionIds()).toEqual(['s-live'])
  })

  it('hands a foreign tab to its owner through retainProjectChats and removes it', () => {
    seedTree(['s-own', 's-foreign'])
    prune({ 's-own': 'p1', 's-foreign': 'p2' })
    expect(treeSessionIds()).toEqual(['s-own'])
    expect(useAgentChatLifetimeStore.getState().retainedByProject).toEqual({ p2: ['s-foreign'] })
    expect(logFrontendError).toHaveBeenCalledTimes(1)
    expect(logFrontendError).toHaveBeenCalledWith(
      expect.objectContaining({ level: 'warn', source: 'acp-store.loadSessionIndex' })
    )
  })

  it('keeps the tab and does not retain when the owner is not in the project list', () => {
    seedTree(['s-orphan'])
    prune({ 's-orphan': 'px' })
    expect(treeSessionIds()).toEqual(['s-orphan'])
    expect(useAgentChatLifetimeStore.getState().retainedByProject).toEqual({})
    expect(logFrontendError).toHaveBeenCalledTimes(1)
    expect(vi.mocked(logFrontendError).mock.calls[0][0].message).toContain('s-orphan')
    expect(vi.mocked(logFrontendError).mock.calls[0][0].message).toContain('px')
  })

  it('keeps the tab when the owner is archived', () => {
    useProjectStore.setState({ projects: [project('p1'), project('p2', true)] })
    seedTree(['s-foreign'])
    prune({ 's-foreign': 'p2' })
    expect(treeSessionIds()).toEqual(['s-foreign'])
    expect(useAgentChatLifetimeStore.getState().retainedByProject).toEqual({})
  })

  it('keeps the tab while the project list is not loaded yet', () => {
    useProjectStore.setState({ projects: [] })
    seedTree(['s-foreign'])
    prune({ 's-foreign': 'p2' })
    expect(treeSessionIds()).toEqual(['s-foreign'])
    expect(useAgentChatLifetimeStore.getState().retainedByProject).toEqual({})
  })

  it('keeps a tab with unknown or stub ownership without a log', () => {
    seedTree(['s-stub'])
    prune({ 's-stub': '' })
    expect(treeSessionIds()).toEqual(['s-stub'])
    expect(logFrontendError).not.toHaveBeenCalled()
  })
})
