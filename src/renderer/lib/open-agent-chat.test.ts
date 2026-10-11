import { beforeEach, describe, expect, it, vi } from 'vitest'

const {
  mockIsMobile,
  mockAddAgentChatTab,
  mockMoveTabToPane,
  mockMoveTabToNewSplit,
  mockSelectProfile,
  acpRef,
  wsRef
} = vi.hoisted(() => ({
  mockIsMobile: vi.fn(() => false),
  mockAddAgentChatTab: vi.fn(),
  mockMoveTabToPane: vi.fn(),
  mockMoveTabToNewSplit: vi.fn(),
  mockSelectProfile: vi.fn(),
  acpRef: {
    current: {
      sessions: {} as Record<string, { projectId: string }>,
      sessionIndex: [] as Array<{ id: string; projectId: string }>,
      switchProject: vi.fn(async () => ({ status: 'selected' })),
      setFailedProjectSwitch: vi.fn()
    }
  },
  wsRef: {
    root: { type: 'leaf', id: 'pane-1', tabs: [], activeTabId: null } as FakePane
  }
}))

interface FakePane {
  type: 'leaf' | 'split'
  id: string
  tabs?: Array<{ id: string }>
  children?: FakePane[]
  activeTabId?: string | null
}

function leavesOf(node: FakePane): FakePane[] {
  return node.type === 'leaf' ? [node] : (node.children ?? []).flatMap(leavesOf)
}

vi.mock('@/hooks/use-mobile-web-shell', () => ({ isMobileWebShellViewport: mockIsMobile }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('@/stores/acp-store', () => ({ useAcpStore: { getState: () => acpRef.current } }))
vi.mock('@/stores/workspace-store', () => ({
  agentChatTabId: (sessionId: string) => `chat-${sessionId}`,
  findPaneById: (root: FakePane, id: string): FakePane | null =>
    leavesOf(root).find((leaf) => leaf.id === id) ?? null,
  findPaneContainingTab: (root: FakePane, tabId: string): FakePane | null =>
    leavesOf(root).find((leaf) => (leaf.tabs ?? []).some((tab) => tab.id === tabId)) ?? null,
  useWorkspaceStore: {
    getState: () => ({
      root: wsRef.root,
      addAgentChatTab: mockAddAgentChatTab,
      moveTabToPane: mockMoveTabToPane,
      moveTabToNewSplit: mockMoveTabToNewSplit
    })
  }
}))
vi.mock('@/stores/ssh-store', () => ({
  useSSHStore: { getState: () => ({ selectProfile: mockSelectProfile }) }
}))

import { logFrontendError } from '@/lib/log-api'
import { useAgentChatLifetimeStore } from '@/stores/agent-chat-lifetime-store'
import { useProjectStore } from '@/stores/project-store'
import { openAgentChatInOwnProject } from './open-agent-chat'

describe('openAgentChatInOwnProject', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockIsMobile.mockReturnValue(false)
    acpRef.current.sessions = { 's-a': { projectId: 'pa' }, 's-b': { projectId: 'pb' } }
    acpRef.current.sessionIndex = []
    wsRef.root = { type: 'leaf', id: 'pane-1', tabs: [], activeTabId: null }
    useProjectStore.setState({ activeProjectId: 'pa' })
    useAgentChatLifetimeStore.setState({ retainedByProject: {}, focusSessionByProject: {} })
  })

  it('opens a chat of the active project in place', () => {
    expect(openAgentChatInOwnProject('s-a', 'test')).toBe('opened')
    expect(mockAddAgentChatTab).toHaveBeenCalledWith('s-a')
    expect(useProjectStore.getState().activeProjectId).toBe('pa')
  })

  it('opens a chat with unknown ownership in place (fail-open)', () => {
    openAgentChatInOwnProject('s-unknown', 'test')
    expect(mockAddAgentChatTab).toHaveBeenCalledWith('s-unknown')
  })

  it('switches to the owning project instead of adding the tab here', () => {
    expect(openAgentChatInOwnProject('s-b', 'test')).toBe('switched-project')
    expect(mockAddAgentChatTab).not.toHaveBeenCalled()
    expect(useProjectStore.getState().activeProjectId).toBe('pb')
    expect(mockSelectProfile).toHaveBeenCalledWith(null)
    const lifetime = useAgentChatLifetimeStore.getState()
    expect(lifetime.retainedByProject.pb).toEqual(['s-b'])
    expect(lifetime.focusSessionByProject.pb).toBe('s-b')
  })

  it('resolves the owner from the history index for closed chats', () => {
    acpRef.current.sessionIndex = [{ id: 's-closed', projectId: 'pb' }]
    openAgentChatInOwnProject('s-closed', 'test')
    expect(useProjectStore.getState().activeProjectId).toBe('pb')
  })

  it('switches through the server session on the phone shell', () => {
    mockIsMobile.mockReturnValue(true)
    expect(openAgentChatInOwnProject('s-b', 'test')).toBe('switched-project')
    expect(acpRef.current.switchProject).toHaveBeenCalledWith('pb')
    expect(mockAddAgentChatTab).not.toHaveBeenCalled()
    expect(useAgentChatLifetimeStore.getState().focusSessionByProject.pb).toBe('s-b')
  })

  describe('with a placement', () => {
    it('creates the tab in the target pane for a center drop', () => {
      expect(openAgentChatInOwnProject('s-a', 'test', { paneId: 'pane-1', zone: 'center' })).toBe(
        'opened'
      )
      expect(mockAddAgentChatTab).toHaveBeenCalledWith('s-a', 'pane-1')
      expect(mockMoveTabToPane).not.toHaveBeenCalled()
      expect(mockMoveTabToNewSplit).not.toHaveBeenCalled()
    })

    it('splits the pane for an edge drop when other tabs share it', () => {
      wsRef.root = {
        type: 'leaf',
        id: 'pane-1',
        tabs: [{ id: 'chat-s-a' }, { id: 'editor-x' }],
        activeTabId: 'chat-s-a'
      }
      openAgentChatInOwnProject('s-a', 'test', { paneId: 'pane-1', zone: 'right' })
      expect(mockMoveTabToNewSplit).toHaveBeenCalledWith('chat-s-a', 'pane-1', 'pane-1', 'right')
    })

    it('does not split a pane off its own only tab', () => {
      wsRef.root = {
        type: 'leaf',
        id: 'pane-1',
        tabs: [{ id: 'chat-s-a' }],
        activeTabId: 'chat-s-a'
      }
      openAgentChatInOwnProject('s-a', 'test', { paneId: 'pane-1', zone: 'bottom' })
      expect(mockMoveTabToNewSplit).not.toHaveBeenCalled()
      expect(mockMoveTabToPane).not.toHaveBeenCalled()
    })

    it('moves an existing tab from another pane to a center drop', () => {
      wsRef.root = {
        type: 'split',
        id: 'split-1',
        children: [
          { type: 'leaf', id: 'pane-1', tabs: [{ id: 'chat-s-a' }], activeTabId: 'chat-s-a' },
          { type: 'leaf', id: 'pane-2', tabs: [{ id: 'editor-x' }], activeTabId: 'editor-x' }
        ]
      }
      openAgentChatInOwnProject('s-a', 'test', { paneId: 'pane-2', zone: 'center' })
      expect(mockMoveTabToPane).toHaveBeenCalledWith('chat-s-a', 'pane-1', 'pane-2')
      expect(mockMoveTabToNewSplit).not.toHaveBeenCalled()
    })

    it('falls back to open-in-place when the pane is missing', () => {
      expect(openAgentChatInOwnProject('s-a', 'test', { paneId: 'gone', zone: 'left' })).toBe(
        'opened'
      )
      expect(mockAddAgentChatTab).toHaveBeenCalledWith('s-a')
      expect(mockMoveTabToNewSplit).not.toHaveBeenCalled()
    })

    it('rejects a foreign chat without placing it, switching or retaining', () => {
      expect(openAgentChatInOwnProject('s-b', 'test', { paneId: 'pane-1', zone: 'center' })).toBe(
        'rejected'
      )
      expect(mockAddAgentChatTab).not.toHaveBeenCalled()
      expect(useProjectStore.getState().activeProjectId).toBe('pa')
      expect(useAgentChatLifetimeStore.getState().retainedByProject).toEqual({})
      expect(useAgentChatLifetimeStore.getState().focusSessionByProject).toEqual({})
      expect(logFrontendError).toHaveBeenCalledWith(
        expect.objectContaining({ level: 'info', source: 'test' })
      )
    })
  })
})
