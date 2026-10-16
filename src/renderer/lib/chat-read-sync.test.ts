import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import type { AcpTransport } from '@/lib/acp-transport'
import type { WatchHandlers } from '@/lib/acp-transport/watch-client'
import { useAgentChatUnreadStore } from '@/stores/agent-chat-unread-store'
import { markChatRead, startChatReadSync } from './chat-read-sync'

type EventCallback = (payload: unknown, eventSeq?: number) => void

function fakeSurface(
  markChatReadImpl = vi.fn(async (sessionId: string, seq: number) => ({ sessionId, seq }))
) {
  const listeners = new Map<string, EventCallback>()
  let watch: WatchHandlers | null = null
  const transport = {
    onEvent: (name: string, callback: EventCallback) => {
      listeners.set(name, callback)
      return () => listeners.delete(name)
    },
    watches: {
      subscribe: (name: string, handlers: WatchHandlers) => {
        expect(name).toBe('chat_reads')
        watch = handlers
        return () => {
          watch = null
        }
      }
    },
    markChatRead: markChatReadImpl
  } as unknown as AcpTransport
  return {
    transport,
    markChatReadImpl,
    turnEnd: (sessionId: string, seq: number) =>
      listeners.get('acp:prompt_complete')?.({ sessionId, stopReason: 'end_turn' }, seq),
    readState: (marks: Record<string, number>) => watch?.onState({ marks }),
    readDelta: (sessionId: string, seq: number) => watch?.onDelta?.({ sessionId, seq }),
    watching: () => watch !== null
  }
}

function unreadIds(): string[] {
  return Object.keys(useAgentChatUnreadStore.getState().unread).sort()
}

beforeEach(() => {
  useAgentChatUnreadStore.setState({ unread: {}, seenTurnEndSeq: {}, sharedReadSeq: {} })
})

describe('startChatReadSync', () => {
  it('clears the dot banked here when another surface reads the same turn end', () => {
    const desktop = fakeSurface()
    const stop = startChatReadSync(desktop.transport)
    desktop.readState({})
    desktop.turnEnd('s1', 7)
    desktop.turnEnd('s2', 9)
    useAgentChatUnreadStore.getState().markUnread('s1')
    useAgentChatUnreadStore.getState().markUnread('s2')

    desktop.readDelta('s1', 7)

    expect(unreadIds()).toEqual(['s2'])
    stop()
    expect(desktop.watching()).toBe(false)
  })

  it('keeps the dot when the other surface is behind the newest turn end', () => {
    const desktop = fakeSurface()
    startChatReadSync(desktop.transport)
    desktop.turnEnd('s1', 7)
    desktop.turnEnd('s1', 15)
    useAgentChatUnreadStore.getState().markUnread('s1')

    desktop.readState({ s1: 7 })

    expect(unreadIds()).toEqual(['s1'])
  })

  it('ignores a turn end that carries no seq', () => {
    const desktop = fakeSurface()
    startChatReadSync(desktop.transport)
    desktop.turnEnd('s1', 0)

    expect(useAgentChatUnreadStore.getState().seenTurnEndSeq).toEqual({})
  })
})

describe('markChatRead', () => {
  it('sends a seq once while it is in flight and again after a failure', async () => {
    const failing = vi.fn(async () => {
      throw new Error('host gone')
    })
    const phone = fakeSurface(failing)

    markChatRead('s1', 7, phone.transport)
    markChatRead('s1', 7, phone.transport)
    markChatRead('s1', 5, phone.transport)
    expect(failing).toHaveBeenCalledTimes(1)
    await new Promise((resolve) => setTimeout(resolve, 0))

    markChatRead('s1', 7, phone.transport)
    expect(failing).toHaveBeenCalledTimes(2)
    expect(failing).toHaveBeenLastCalledWith('s1', 7)
  })
})
