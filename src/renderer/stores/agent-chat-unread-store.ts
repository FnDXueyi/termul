import { create } from 'zustand'

interface AgentChatUnreadState {
  /** Sessions whose turn finished while they were not the active chat ("New activity"). */
  unread: Record<string, true>
  markUnread: (sessionId: string) => void
  clearUnread: (sessionId: string) => void
}

/**
 * The single owner of the per-session "Done" ("New activity") flag. The desktop
 * `AgentChatTabInline` and the mobile Recents rows both read it, so a chat's
 * tab and card can never disagree. It is set and cleared by the one shell-level
 * `useAgentChatUnreadTracker` mount (`useShellUnreadTracker`), plus
 * `deleteHistorySession`.
 *
 * Lifetime: a flag is set when a turn finishes (or is cancelled) while the chat
 * is no pane's active tab; it clears only when the chat becomes a pane's active
 * tab or the chat is deleted. It survives the session leaving the live set and
 * project switches. Never persisted, so an app restart resets it.
 */
export const useAgentChatUnreadStore = create<AgentChatUnreadState>((set) => ({
  unread: {},
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
  }
}))
