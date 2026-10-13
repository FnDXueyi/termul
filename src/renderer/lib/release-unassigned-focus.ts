import { UNASSIGNED_PROJECT_ID } from '@shared/types/web-projects.types'
import { getTabFocusedSessionId, setTabFocusedSessionId } from '@/lib/web-tab-session'
import { useAcpStore } from '@/stores/acp-store'

/**
 * Whether `sessionId` is stamped unassigned by the host: it is listed
 * everywhere but retained by no project, so no project's manifest may inherit
 * it as its focus.
 */
function isUnassigned(
  sessionId: string,
  acp: Pick<ReturnType<typeof useAcpStore.getState>, 'sessions' | 'sessionIndex'>
): boolean {
  const projectId =
    acp.sessions[sessionId]?.projectId ||
    acp.sessionIndex.find((entry) => entry.id === sessionId)?.projectId
  return projectId === UNASSIGNED_PROJECT_ID
}

/**
 * Clears the active-session and tab-focus pointers when they still name an
 * unassigned chat. Run before selecting a project so a later manifest write
 * cannot persist that chat as the selected project's focus.
 */
export function releaseUnassignedFocus(): void {
  const acp = useAcpStore.getState()
  if (acp.activeSessionId && isUnassigned(acp.activeSessionId, acp)) {
    useAcpStore.setState({ activeSessionId: null })
  }
  const tabFocused = getTabFocusedSessionId()
  if (tabFocused && isUnassigned(tabFocused, acp)) {
    setTabFocusedSessionId(null)
  }
}
