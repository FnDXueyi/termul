import { useEffect, useSyncExternalStore } from 'react'
import { markChatRead } from '@/lib/chat-read-sync'
import { useAgentChatUnreadStore } from '@/stores/agent-chat-unread-store'

function subscribeVisibility(onChange: () => void): () => void {
  document.addEventListener('visibilitychange', onChange)
  return () => document.removeEventListener('visibilitychange', onChange)
}

function pageVisible(): boolean {
  return document.visibilityState === 'visible'
}

/**
 * Reports every shown chat as read on the host, so the dot it banked on other
 * surfaces clears: on becoming shown, and on each turn end while shown. A
 * hidden page (a backgrounded phone tab, a hidden window) reads nothing, so a
 * phone left on a chat never clears the desktop's dot. `activeSessionIds` is
 * the chats this renderer shows (`activeChatSessionIds`).
 */
export function useChatReadMarker(activeSessionIds: readonly string[]): void {
  const visible = useSyncExternalStore(subscribeVisibility, pageVisible)
  const seenTurnEndSeq = useAgentChatUnreadStore((state) => state.seenTurnEndSeq)
  const sharedReadSeq = useAgentChatUnreadStore((state) => state.sharedReadSeq)

  useEffect(() => {
    if (!visible) return
    for (const sessionId of activeSessionIds) {
      const seq = seenTurnEndSeq[sessionId]
      if (seq !== undefined && seq > (sharedReadSeq[sessionId] ?? 0)) markChatRead(sessionId, seq)
    }
  }, [activeSessionIds, visible, seenTurnEndSeq, sharedReadSeq])
}
