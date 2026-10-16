import { create } from 'zustand'

interface AgentChatUnreadState {
  /** Sessions whose turn finished while they were not the active chat ("New activity"). */
  unread: Record<string, true>
  /** Host-stamped session-log seq of the newest turn end this renderer has seen. */
  seenTurnEndSeq: Record<string, number>
  /**
   * The host's shared last-read marker: the newest seq any surface (this one,
   * a shared-live phone, another browser) has read. Mirrors the `chat_reads` watch.
   */
  sharedReadSeq: Record<string, number>
  markUnread: (sessionId: string) => void
  clearUnread: (sessionId: string) => void
  noteTurnEnd: (sessionId: string, seq: number) => void
  setReadMarks: (marks: Record<string, number>) => void
  setReadMark: (sessionId: string, seq: number) => void
}

type UnreadMaps = Pick<AgentChatUnreadState, 'unread' | 'seenTurnEndSeq' | 'sharedReadSeq'>

function readElsewhere(state: UnreadMaps, sessionId: string): boolean {
  const turnEnd = state.seenTurnEndSeq[sessionId]
  const read = state.sharedReadSeq[sessionId]
  return turnEnd !== undefined && read !== undefined && read >= turnEnd
}

function withoutReadFlags(state: UnreadMaps): Record<string, true> {
  let unread = state.unread
  for (const sessionId of Object.keys(state.unread)) {
    if (!readElsewhere(state, sessionId)) continue
    if (unread === state.unread) unread = { ...state.unread }
    delete unread[sessionId]
  }
  return unread
}

/**
 * The single owner of the per-session "Done" ("New activity") flag. The desktop
 * `AgentChatTabInline` and the mobile Recents rows both read it, so a chat's
 * tab and card can never disagree. It is set and cleared by the one shell-level
 * `useAgentChatUnreadTracker` mount (`useShellUnreadTracker`), plus
 * `deleteHistorySession`.
 *
 * Lifetime: a flag is set when a turn finishes (or is cancelled) while the chat
 * is no pane's active tab. It clears when the chat becomes a pane's active tab,
 * when the host's shared last-read marker reaches the newest turn end seen here
 * (the chat was read on another surface), or when the chat is deleted. It
 * survives the session leaving the live set and project switches. Never
 * persisted, so an app restart resets it.
 */
export const useAgentChatUnreadStore = create<AgentChatUnreadState>((set) => ({
  unread: {},
  seenTurnEndSeq: {},
  sharedReadSeq: {},
  markUnread: (sessionId) => {
    if (!sessionId) return
    set((state) =>
      state.unread[sessionId] ? state : { unread: { ...state.unread, [sessionId]: true } }
    )
  },
  clearUnread: (sessionId) => {
    set((state) => {
      if (!state.unread[sessionId]) return state
      const unread = { ...state.unread }
      delete unread[sessionId]
      return { unread }
    })
  },
  noteTurnEnd: (sessionId, seq) => {
    set((state) => {
      if ((state.seenTurnEndSeq[sessionId] ?? 0) >= seq) return state
      const next = { ...state, seenTurnEndSeq: { ...state.seenTurnEndSeq, [sessionId]: seq } }
      return { seenTurnEndSeq: next.seenTurnEndSeq, unread: withoutReadFlags(next) }
    })
  },
  setReadMarks: (marks) => {
    set((state) => {
      const next = { ...state, sharedReadSeq: { ...marks } }
      return { sharedReadSeq: next.sharedReadSeq, unread: withoutReadFlags(next) }
    })
  },
  setReadMark: (sessionId, seq) => {
    set((state) => {
      if (state.sharedReadSeq[sessionId] === seq) return state
      const next = { ...state, sharedReadSeq: { ...state.sharedReadSeq, [sessionId]: seq } }
      return { sharedReadSeq: next.sharedReadSeq, unread: withoutReadFlags(next) }
    })
  }
}))
