/**
 * Renderer facade for the host's shared chat last-read markers. Every carrier
 * speaks `/ws` (the desktop over its IPC Channel), so one client serves the
 * desktop, a shared-live phone and a `termul-server` browser alike.
 */
import type { ChatReadsWatchDelta, ChatReadsWatchState } from '@shared/types/web-protocol.types'
import { ACP_EVENTS, type PromptCompleteEvent } from '@/lib/acp-api'
import { type AcpTransport, getAcpTransport } from '@/lib/acp-transport'
import { logFrontendError } from '@/lib/log-api'
import { useAgentChatUnreadStore } from '@/stores/agent-chat-unread-store'

const SOURCE = 'chat-read-sync'

const unansweredSeq = new Map<string, number>()

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/** Start mirroring turn ends and the host's read markers. Returns the stop. */
export function startChatReadSync(transport: AcpTransport = getAcpTransport()): () => void {
  const store = useAgentChatUnreadStore.getState
  const offTurnEnds = transport.onEvent<PromptCompleteEvent>(
    ACP_EVENTS.promptComplete,
    (event, eventSeq) => {
      if (typeof eventSeq === 'number' && eventSeq > 0 && event?.sessionId) {
        store().noteTurnEnd(event.sessionId, eventSeq)
      }
    }
  )
  const offMarks = transport.watches?.subscribe<ChatReadsWatchState, ChatReadsWatchDelta>(
    'chat_reads',
    {
      onState: (state) => store().setReadMarks(state?.marks ?? {}),
      onDelta: (delta) => {
        if (delta?.sessionId && typeof delta.seq === 'number') {
          store().setReadMark(delta.sessionId, delta.seq)
        }
      },
      onRefused: (error) => {
        void logFrontendError({
          level: 'warn',
          source: SOURCE,
          message: `chat_reads watch refused; reads on other surfaces will not clear dots here: ${describe(error)}`
        })
      }
    }
  )
  return () => {
    offTurnEnds()
    offMarks?.()
  }
}

/**
 * Tell the host this surface read `sessionId` up to `seq`. A seq already in
 * flight for the session is not sent again; a failure is logged and lets the
 * next call retry.
 */
export function markChatRead(
  sessionId: string,
  seq: number,
  transport: AcpTransport = getAcpTransport()
): void {
  if (!transport.markChatRead || (unansweredSeq.get(sessionId) ?? 0) >= seq) return
  unansweredSeq.set(sessionId, seq)
  const settle = () => {
    if (unansweredSeq.get(sessionId) === seq) unansweredSeq.delete(sessionId)
  }
  transport.markChatRead(sessionId, seq).then(settle, (error: unknown) => {
    settle()
    void logFrontendError({
      level: 'warn',
      source: SOURCE,
      message: `mark_chat_read failed for a chat at seq ${seq}: ${describe(error)}`
    })
  })
}
