import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('sonner', () => ({
  toast: { error: vi.fn(), warning: vi.fn() }
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn()
}))
vi.mock('@/lib/tauri-runtime', () => ({
  isTauriContext: vi.fn(() => false),
  cleanupTauriListener: vi.fn()
}))
vi.mock('@/lib/log-api', () => ({
  logFrontendError: vi.fn()
}))
vi.mock('@/lib/acp-agents-persistence', async (orig) => {
  const actual = await orig<typeof import('@/lib/acp-agents-persistence')>()
  return {
    ...actual,
    loadAgentConfigs: vi.fn(async () => []),
    saveAgentConfigs: vi.fn(async () => {})
  }
})
vi.mock('@/lib/acp-history-persistence', async (orig) => {
  const actual = await orig<typeof import('@/lib/acp-history-persistence')>()
  return {
    ...actual,
    loadSessionIndex: vi.fn(async () => []),
    saveSessionIndex: vi.fn(async () => {}),
    saveSessionPayload: vi.fn(async () => {}),
    queueSessionPayloadSave: vi.fn(async () => {}),
    queueSessionPayloadDelete: vi.fn(async () => {}),
    loadSessionPayload: actual.loadSessionPayload,
    loadSessionPayloadTail: vi.fn(async () => null)
  }
})
vi.mock('@/lib/acp-mcp-persistence', async (orig) => {
  const actual = await orig<typeof import('@/lib/acp-mcp-persistence')>()
  return {
    ...actual,
    loadMcpServers: vi.fn(async () => []),
    saveMcpServers: vi.fn(async () => {}),
    syncMcpRegistryToProjectBestEffort: vi.fn(async () => {})
  }
})

vi.mock('@/stores/workspace-store', () => ({
  agentChatTabId: (sessionId: string) => `chat-${sessionId}`,
  getAllLeafPanes: () => [],
  findPaneContainingTab: () => null,
  useWorkspaceStore: {
    getState: () => ({ addAgentChatTab: vi.fn(), root: null })
  }
}))

vi.mock('@/lib/web-tab-session', () => ({
  setTabFocusedSessionId: vi.fn(),
  getTabFocusedSessionId: vi.fn(() => null)
}))

const { mockPersistenceApi } = vi.hoisted(() => ({
  mockPersistenceApi: {
    read: vi.fn(async () => ({ success: false })),
    write: vi.fn(),
    writeDebounced: vi.fn(async () => ({ success: true })),
    delete: vi.fn(async () => ({ success: true }))
  }
}))
vi.mock('@/lib/api', async (importActual) => {
  const actual = await importActual<typeof import('@/lib/api')>()
  return {
    ...actual,
    persistenceApi: mockPersistenceApi
  }
})

import type { AcpTransport } from '@/lib/acp-transport'
import { _resetAcpTransportForTests, _setAcpTransportForTests } from '@/lib/acp-transport'
import { isTauriContext } from '@/lib/tauri-runtime'
import {
  _resetAcpAuthForTesting,
  _resetEphemeralSessionIdsForTesting,
  _resetHistorySeqWatermarksForTesting,
  _resetInFlightHistoryOpensForTesting,
  _resetInFlightPreparedForTesting,
  _resetLiveSwitchSourcesForTesting,
  _resetSessionIndexLoadGenerationForTesting,
  agentReuseKey,
  useAcpStore
} from '@/stores/acp-store'
import { FRESH } from './testkit'

const CONFIG_ID = 'acp-registry:claude-acp'
const OWNER = 'agent-owner'
const FRESH_AGENT = 'agent-fresh'
const SESSION = 's1'

type FakeTransport = AcpTransport & {
  loadSession: ReturnType<typeof vi.fn>
  sendPrompt: ReturnType<typeof vi.fn>
  closeSession: ReturnType<typeof vi.fn>
  subscribeSession: ReturnType<typeof vi.fn>
  spawnAgent: ReturnType<typeof vi.fn>
}

function fakeWebTransport(
  summaries: Array<Record<string, unknown>>,
  payloads: Record<string, unknown>
): FakeTransport {
  return {
    listAgentDetails: async () => summaries,
    spawnAgent: vi.fn(async () => ({
      agentId: FRESH_AGENT,
      capabilities: { loadSession: true }
    })),
    loadSession: vi.fn(async () => ({})),
    resumeSession: vi.fn(async () => ({})),
    sendPrompt: vi.fn(async () => 'end_turn'),
    sendPromptBlocks: vi.fn(async () => 'end_turn'),
    closeSession: vi.fn(async () => {}),
    killAgent: vi.fn(async () => {}),
    getSessionPayload: async (id: string) => payloads[id] ?? null,
    getSessionPayloadTail: async () => null,
    subscribeSession: vi.fn(async () => {}),
    seedSessionCursor: vi.fn(),
    connect: async () => {},
    dispose: () => {},
    historyMode: () => 'server' as const
  } as unknown as FakeTransport
}

function persistedRecord(status: 'active' | 'error'): Record<string, unknown> {
  return {
    metadata: {
      id: SESSION,
      agentId: OWNER,
      agentConfigId: CONFIG_ID,
      title: 'Chat',
      cwd: '/w',
      projectId: 'p1',
      createdAt: 1,
      lastActivityAt: 2,
      messageCount: 1,
      lastSeq: 1,
      status
    },
    messages: [
      {
        id: 'm1',
        role: 'user',
        blocks: [{ type: 'text', text: 'do the thing' }],
        streaming: false,
        timestamp: 0,
        seq: 1
      }
    ]
  }
}

function ownerSummary(): Record<string, unknown> {
  return {
    id: OWNER,
    name: 'Claude',
    configId: CONFIG_ID,
    capabilities: { loadSession: true },
    ownsSession: [SESSION]
  }
}

function seedLiveTurn(): void {
  useAcpStore.setState({
    agentConfigs: [
      {
        id: CONFIG_ID,
        name: 'Claude',
        command: 'claude',
        args: [],
        env: {},
        templateId: 'claude-acp'
      }
    ],
    agents: { [OWNER]: { id: OWNER, capabilities: { loadSession: true } } },
    agentStatus: { [OWNER]: 'connected' },
    sessions: {
      [SESSION]: {
        id: SESSION,
        agentId: OWNER,
        cwd: '/w',
        projectId: 'p1',
        status: 'active',
        title: 'Chat',
        activeTurn: true,
        openTurnId: 'turn:m1',
        modes: null,
        models: null,
        configOptions: [],
        lastError: null,
        createdAt: 1
      }
    },
    messages: {
      [SESSION]: [
        {
          id: 'm1',
          role: 'user',
          blocks: [{ type: 'text', text: 'do the thing' }],
          streaming: false,
          timestamp: 0,
          seq: 1
        }
      ]
    },
    sessionIndex: [
      {
        id: SESSION,
        agentId: OWNER,
        agentConfigId: CONFIG_ID,
        title: 'Chat',
        cwd: '/w',
        projectId: 'p1',
        createdAt: 1,
        lastActivityAt: 2,
        messageCount: 1,
        status: 'active'
      }
    ]
  })
}

function failTurn(): void {
  useAcpStore.getState()._onAgentError({
    agentId: OWNER,
    sessionId: SESSION,
    message: 'Internal error',
    code: -32603,
    data: { detail: 'boom' }
  })
  useAcpStore.getState()._onPromptComplete({
    agentId: OWNER,
    sessionId: SESSION,
    stopReason: 'error'
  })
}

describe('acp-store: a turn-scoped agent_error keeps the chat on its live agent (#1190)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(isTauriContext).mockReturnValue(false)
    _resetAcpTransportForTests(null)
    _resetInFlightHistoryOpensForTesting()
    _resetAcpAuthForTesting()
    _resetInFlightPreparedForTesting()
    _resetEphemeralSessionIdsForTesting()
    _resetSessionIndexLoadGenerationForTesting()
    _resetHistorySeqWatermarksForTesting()
    _resetLiveSwitchSourcesForTesting()
    useAcpStore.setState(FRESH)
    seedLiveTurn()
  })

  it('web: the reopen keeps the error and never re-attaches to the finished turn', async () => {
    const transport = fakeWebTransport([ownerSummary()], { [SESSION]: persistedRecord('active') })
    _setAcpTransportForTests(transport)

    failTurn()
    await useAcpStore.getState().openHistorySession(SESSION)

    const session = useAcpStore.getState().sessions[SESSION]
    expect(session.lastError).toBe('Internal error')
    expect(session.lastErrorCode).toBe(-32603)
    expect(session.lastErrorData).toEqual({ detail: 'boom' })
    expect(session.activeTurn).toBe(false)
    expect(session.openTurnId).toBeNull()
    expect(session.status).toBe('active')
    expect(session.agentId).toBe(OWNER)
    expect(useAcpStore.getState().agentStatus[OWNER]).toBe('connected')
    expect(transport.subscribeSession).not.toHaveBeenCalled()
    expect(transport.spawnAgent).not.toHaveBeenCalled()

    await useAcpStore.getState().sendPrompt(SESSION, 'try again')
    expect(transport.sendPrompt).toHaveBeenCalledWith(
      OWNER,
      SESSION,
      'try again',
      expect.any(String),
      undefined
    )
  })

  it('desktop: the reopen keeps the error and never spawns a replacement agent', async () => {
    vi.mocked(isTauriContext).mockReturnValue(true)
    const transport = fakeWebTransport([], { [SESSION]: persistedRecord('active') })
    _setAcpTransportForTests(transport)

    failTurn()
    await useAcpStore.getState().openHistorySession(SESSION)

    const session = useAcpStore.getState().sessions[SESSION]
    expect(session.lastError).toBe('Internal error')
    expect(session.lastErrorCode).toBe(-32603)
    expect(session.lastErrorData).toEqual({ detail: 'boom' })
    expect(session.activeTurn).toBe(false)
    expect(session.status).toBe('active')
    expect(session.agentId).toBe(OWNER)
    expect(useAcpStore.getState().agentStatus[OWNER]).toBe('connected')
    expect(transport.spawnAgent).not.toHaveBeenCalled()
    expect(transport.loadSession).not.toHaveBeenCalled()
  })

  it('agent_crashed still runs the dead-agent reopen onto a fresh agent', async () => {
    const transport = fakeWebTransport([], { [SESSION]: persistedRecord('error') })
    _setAcpTransportForTests(transport)

    useAcpStore.getState()._onAgentCrashed({
      agentId: OWNER,
      sessionId: SESSION,
      message: 'child exited: signal 11'
    })
    useAcpStore.getState()._onAgentError({ agentId: OWNER, message: 'child exited: signal 11' })
    useAcpStore.getState()._onAgentDisconnected({ agentId: OWNER, reason: 'crashed' })
    expect(useAcpStore.getState().sessions[SESSION].status).toBe('error')
    expect(useAcpStore.getState().sessions[SESSION].lastError).toBe('child exited: signal 11')

    await useAcpStore.getState().openHistorySession(SESSION)

    const session = useAcpStore.getState().sessions[SESSION]
    expect(transport.spawnAgent).toHaveBeenCalledTimes(1)
    expect(transport.loadSession).toHaveBeenCalledWith(
      FRESH_AGENT,
      SESSION,
      '/w',
      [],
      expect.objectContaining({ commandId: expect.any(String) })
    )
    expect(session.status).toBe('active')
    expect(session.agentId).toBe(FRESH_AGENT)
    expect(session.activeTurn).toBe(false)
    expect(session.lastError).toBeNull()
  })
})

describe('acp-store: a turn error that leaves the agent unusable retires it from reuse', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(isTauriContext).mockReturnValue(true)
    _resetAcpTransportForTests(null)
    _resetInFlightHistoryOpensForTesting()
    _resetAcpAuthForTesting()
    _resetInFlightPreparedForTesting()
    _resetEphemeralSessionIdsForTesting()
    _resetSessionIndexLoadGenerationForTesting()
    _resetHistorySeqWatermarksForTesting()
    _resetLiveSwitchSourcesForTesting()
    useAcpStore.setState(FRESH)
    seedLiveTurn()
    useAcpStore.setState({ configToLiveAgent: { [agentReuseKey(CONFIG_ID, '/w')]: OWNER } })
  })

  async function closeChatThenStartAnother(transport: FakeTransport): Promise<void> {
    await useAcpStore.getState().openHistorySession(SESSION)
    expect(transport.spawnAgent).not.toHaveBeenCalled()
    await useAcpStore.getState().closeSession(SESSION)
    await useAcpStore.getState().prewarmAgent(CONFIG_ID, '/w')
  }

  it('auth required (-32000): the banner stays, the chat stays live, and a new chat spawns a fresh agent', async () => {
    const transport = fakeWebTransport([], { [SESSION]: persistedRecord('active') })
    _setAcpTransportForTests(transport)

    useAcpStore.getState()._onAgentError({
      agentId: OWNER,
      sessionId: SESSION,
      message: 'Authentication required',
      code: -32000
    })
    useAcpStore.getState()._onPromptComplete({
      agentId: OWNER,
      sessionId: SESSION,
      stopReason: 'error'
    })

    const session = useAcpStore.getState().sessions[SESSION]
    expect(session.status).toBe('active')
    expect(session.lastError).toBe('Authentication required')
    expect(session.lastErrorCode).toBe(-32000)
    expect(session.activeTurn).toBe(false)

    await closeChatThenStartAnother(transport)
    expect(transport.spawnAgent).toHaveBeenCalledTimes(1)
  })

  it('turn timeout: the banner stays, the chat stays live, and a new chat spawns a fresh agent', async () => {
    const transport = fakeWebTransport([], { [SESSION]: persistedRecord('active') })
    _setAcpTransportForTests(transport)

    useAcpStore.getState()._onAgentError({
      agentId: OWNER,
      sessionId: SESSION,
      message: 'turn idle timeout: no agent activity for 5s'
    })
    useAcpStore.getState()._onPromptComplete({
      agentId: OWNER,
      sessionId: SESSION,
      stopReason: 'timeout'
    })

    const session = useAcpStore.getState().sessions[SESSION]
    expect(session.status).toBe('active')
    expect(session.lastError).toBe('turn idle timeout: no agent activity for 5s')
    expect(session.activeTurn).toBe(false)

    await closeChatThenStartAnother(transport)
    expect(transport.spawnAgent).toHaveBeenCalledTimes(1)
  })

  it('the Retry restart (stop the retired agent, then reopen) resends on a fresh agent', async () => {
    const transport = fakeWebTransport([], { [SESSION]: persistedRecord('active') })
    _setAcpTransportForTests(transport)

    useAcpStore.getState()._onAgentError({
      agentId: OWNER,
      sessionId: SESSION,
      message: 'Authentication required',
      code: -32000
    })
    useAcpStore.getState()._onPromptComplete({
      agentId: OWNER,
      sessionId: SESSION,
      stopReason: 'error'
    })
    await useAcpStore.getState().killAgent(OWNER)
    await useAcpStore.getState().retryCrashedSession(SESSION)

    const session = useAcpStore.getState().sessions[SESSION]
    expect(transport.spawnAgent).toHaveBeenCalledTimes(1)
    expect(session.status).toBe('active')
    expect(session.agentId).toBe(FRESH_AGENT)
    expect(transport.sendPrompt).toHaveBeenCalledWith(
      FRESH_AGENT,
      SESSION,
      'do the thing',
      expect.any(String),
      expect.anything()
    )
  })

  it('a transient turn error keeps the agent reusable for the next chat', async () => {
    const transport = fakeWebTransport([], { [SESSION]: persistedRecord('active') })
    _setAcpTransportForTests(transport)

    failTurn()

    await closeChatThenStartAnother(transport)
    expect(transport.spawnAgent).not.toHaveBeenCalled()
  })
})
