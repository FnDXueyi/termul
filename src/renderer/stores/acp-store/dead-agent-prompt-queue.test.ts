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
  getAgentChatSessionIdsInAllTrees: () => new Set<string>(),
  findPaneContainingTab: () => null,
  useWorkspaceStore: {
    getState: () => ({ addAgentChatTab: vi.fn(), root: null })
  }
}))
vi.mock('@/lib/web-tab-session', () => ({
  setTabFocusedSessionId: vi.fn(),
  getTabFocusedSessionId: vi.fn(() => null),
  markChatClosedOnRoute: vi.fn()
}))
vi.mock('@/lib/api', async (importActual) => {
  const actual = await importActual<typeof import('@/lib/api')>()
  return {
    ...actual,
    persistenceApi: {
      read: vi.fn(async () => ({ success: false })),
      write: vi.fn(),
      writeDebounced: vi.fn(async () => ({ success: true })),
      delete: vi.fn(async () => ({ success: true }))
    }
  }
})

import type { AcpTransport } from '@/lib/acp-transport'
import { _resetAcpTransportForTests, _setAcpTransportForTests } from '@/lib/acp-transport'
import { closeAgentChat } from '@/lib/agent-chat-close'
import { logFrontendError } from '@/lib/log-api'
import {
  _resetAcpAuthForTesting,
  _resetEphemeralSessionIdsForTesting,
  _resetHistorySeqWatermarksForTesting,
  _resetInFlightHistoryOpensForTesting,
  _resetInFlightPreparedForTesting,
  _resetLiveSwitchSourcesForTesting,
  _resetSessionIndexLoadGenerationForTesting,
  useAcpStore
} from '@/stores/acp-store'
import { FRESH, flushTurnEnd, seedSession } from './testkit'

const CONFIG_ID = 'acp-registry:claude-acp'
const OLD_AGENT = 'agent-old'
const FRESH_AGENT = 'agent-fresh'
const SESSION = 's-queued'

type FakeTransport = AcpTransport & {
  killAgent: ReturnType<typeof vi.fn>
  loadSession: ReturnType<typeof vi.fn>
  sendPrompt: ReturnType<typeof vi.fn>
  sendPromptBlocks: ReturnType<typeof vi.fn>
  spawnAgent: ReturnType<typeof vi.fn>
}

function fakeWebTransport(): FakeTransport {
  return {
    listAgentDetails: async () => [],
    spawnAgent: vi.fn(async () => ({
      agentId: FRESH_AGENT,
      capabilities: { loadSession: true }
    })),
    killAgent: vi.fn(async () => {}),
    loadSession: vi.fn(async () => ({})),
    resumeSession: vi.fn(async () => ({})),
    sendPrompt: vi.fn(async () => 'end_turn'),
    sendPromptBlocks: vi.fn(async () => 'end_turn'),
    getSessionPayload: async (id: string) => (id === SESSION ? closedPayload() : null),
    getSessionPayloadTail: async () => null,
    subscribeSession: vi.fn(async () => {}),
    seedSessionCursor: vi.fn(),
    connect: async () => {},
    dispose: () => {},
    historyMode: () => 'server' as const
  } as unknown as FakeTransport
}

function closedPayload(): Record<string, unknown> {
  return {
    metadata: {
      id: SESSION,
      agentId: OLD_AGENT,
      agentConfigId: CONFIG_ID,
      title: 'Chat',
      cwd: '/work',
      projectId: 'p1',
      createdAt: 1,
      lastActivityAt: 2,
      messageCount: 2,
      lastSeq: 10,
      status: 'closed'
    },
    messages: [
      {
        id: 'm1',
        role: 'user',
        blocks: [{ type: 'text', text: 'refactor the parser' }],
        streaming: false,
        timestamp: 0,
        seq: 1
      },
      {
        id: 'm2',
        role: 'agent',
        blocks: [{ type: 'text', text: 'Done.' }],
        streaming: false,
        timestamp: 1,
        seq: 2
      }
    ]
  }
}

async function queueBehindRunningTurn(): Promise<void> {
  seedSession(SESSION, OLD_AGENT, true)
  await useAcpStore.getState().sendPrompt(SESSION, 'then add tests')
  expect(useAcpStore.getState().promptQueues[SESSION]).toEqual([
    expect.objectContaining({ blocks: [{ type: 'text', text: 'then add tests' }] })
  ])
}

const killAgentPaths = [
  {
    name: 'the host reports the agent gone',
    kill: async () => {
      useAcpStore
        .getState()
        ._onSessionClosed({ agentId: OLD_AGENT, sessionId: SESSION, reason: 'crashed' })
      useAcpStore.getState()._onAgentDisconnected({ agentId: OLD_AGENT, reason: 'crashed' })
    }
  },
  {
    name: 'this client kills the agent',
    kill: () => useAcpStore.getState().killAgent(OLD_AGENT)
  }
]

describe('acp-store: queued prompts when a chat tab closes or its agent dies', () => {
  let transport: FakeTransport

  beforeEach(() => {
    vi.clearAllMocks()
    _resetAcpTransportForTests(null)
    _resetInFlightHistoryOpensForTesting()
    _resetAcpAuthForTesting()
    _resetInFlightPreparedForTesting()
    _resetEphemeralSessionIdsForTesting()
    _resetSessionIndexLoadGenerationForTesting()
    _resetHistorySeqWatermarksForTesting()
    _resetLiveSwitchSourcesForTesting()
    useAcpStore.setState(FRESH)
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
      ]
    })
    transport = fakeWebTransport()
    _setAcpTransportForTests(transport)
  })

  it.each(
    killAgentPaths
  )('drops the queue when $name, so a reopen on a fresh agent sends nothing', async ({ kill }) => {
    await queueBehindRunningTurn()

    await kill()
    await useAcpStore.getState().openHistorySession(SESSION)
    await flushTurnEnd()

    const state = useAcpStore.getState()
    expect(state.sessions[SESSION]).toMatchObject({ agentId: FRESH_AGENT, status: 'active' })
    expect(transport.sendPromptBlocks).not.toHaveBeenCalled()
    expect(transport.sendPrompt).not.toHaveBeenCalled()
    expect(state.promptQueues[SESSION]).toBeUndefined()
    expect(logFrontendError).toHaveBeenCalledWith({
      level: 'info',
      source: 'acp.promptQueue.agentStopped',
      message: `Dropped 1 queued prompt(s) for chat ${SESSION}: agent ${OLD_AGENT} stopped`
    })
  })

  it('keeps the queue when only the tab closes, and sends it when the running turn ends', async () => {
    await queueBehindRunningTurn()
    const closeTab = vi.fn()

    closeAgentChat(SESSION, closeTab)
    await useAcpStore.getState().openHistorySession(SESSION)

    expect(closeTab).toHaveBeenCalledTimes(1)
    expect(useAcpStore.getState().promptQueues[SESSION]).toEqual([
      expect.objectContaining({ blocks: [{ type: 'text', text: 'then add tests' }] })
    ])
    expect(transport.sendPromptBlocks).not.toHaveBeenCalled()

    useAcpStore.getState()._onPromptComplete({
      agentId: OLD_AGENT,
      sessionId: SESSION,
      stopReason: 'end_turn'
    })
    await flushTurnEnd()

    await vi.waitFor(() => {
      expect(transport.sendPromptBlocks).toHaveBeenCalledWith(
        OLD_AGENT,
        SESSION,
        [{ type: 'text', text: 'then add tests' }],
        expect.any(String),
        undefined
      )
    })
    expect(useAcpStore.getState().promptQueues[SESSION]).toEqual([])
  })
})
