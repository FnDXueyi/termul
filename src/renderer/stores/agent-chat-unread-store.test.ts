import { beforeEach, describe, expect, it } from 'vitest'
import { useAgentChatUnreadStore } from './agent-chat-unread-store'

beforeEach(() => {
  useAgentChatUnreadStore.setState({ unread: {}, seenTurnEndSeq: {}, sharedReadSeq: {} })
})

function unread(): Record<string, true> {
  return useAgentChatUnreadStore.getState().unread
}

describe('useAgentChatUnreadStore', () => {
  it('starts with nothing unread', () => {
    expect(useAgentChatUnreadStore.getState().unread).toEqual({})
  })

  it('marks and clears one session without touching the others', () => {
    const { markUnread, clearUnread } = useAgentChatUnreadStore.getState()

    markUnread('a')
    markUnread('b')
    expect(useAgentChatUnreadStore.getState().unread).toEqual({ a: true, b: true })

    clearUnread('a')
    expect(useAgentChatUnreadStore.getState().unread).toEqual({ b: true })
  })

  it('keeps the same state object when marking an already-unread session', () => {
    const { markUnread } = useAgentChatUnreadStore.getState()
    markUnread('a')
    const before = useAgentChatUnreadStore.getState().unread

    markUnread('a')

    // No new record, so subscribers selecting `unread` do not re-render.
    expect(useAgentChatUnreadStore.getState().unread).toBe(before)
  })

  it('keeps the same state object when clearing a session that is not unread', () => {
    useAgentChatUnreadStore.getState().markUnread('a')
    const before = useAgentChatUnreadStore.getState().unread

    useAgentChatUnreadStore.getState().clearUnread('missing')

    expect(useAgentChatUnreadStore.getState().unread).toBe(before)
  })

  it('ignores an empty session id', () => {
    useAgentChatUnreadStore.getState().markUnread('')
    expect(useAgentChatUnreadStore.getState().unread).toEqual({})
  })

  it('clears a dot once another surface read the newest turn end seen here', () => {
    const store = useAgentChatUnreadStore.getState()
    store.noteTurnEnd('a', 12)
    store.markUnread('a')
    store.markUnread('b')

    store.setReadMark('a', 12)

    expect(unread()).toEqual({ b: true })
  })

  it('keeps the dot when the other surface read only an earlier turn', () => {
    const store = useAgentChatUnreadStore.getState()
    store.noteTurnEnd('a', 12)
    store.markUnread('a')

    store.setReadMark('a', 8)

    expect(unread()).toEqual({ a: true })
  })

  it('keeps a dot whose turn end seq it never saw, whatever the marker says', () => {
    const store = useAgentChatUnreadStore.getState()
    store.markUnread('a')

    store.setReadMarks({ a: 500 })

    expect(unread()).toEqual({ a: true })
  })

  it('keeps the dot of a newer turn end than the read, and clears it when the read catches up', () => {
    const store = useAgentChatUnreadStore.getState()
    store.setReadMarks({ a: 12 })
    store.noteTurnEnd('a', 12)
    store.markUnread('a')
    store.noteTurnEnd('a', 30)
    store.setReadMarks({ a: 12 })
    expect(unread()).toEqual({ a: true })

    store.setReadMark('a', 30)

    expect(unread()).toEqual({})
  })

  it('never lowers the newest turn end seen', () => {
    const store = useAgentChatUnreadStore.getState()
    store.noteTurnEnd('a', 30)
    store.noteTurnEnd('a', 12)

    expect(useAgentChatUnreadStore.getState().seenTurnEndSeq).toEqual({ a: 30 })
  })
})
