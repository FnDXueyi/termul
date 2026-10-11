/**
 * Index-load re-filter of restored agent-chat tabs (#903). Runs after the
 * session index lands, over the shared pane tree:
 *  - corpse prune: a tab whose session is neither live nor indexed could only
 *    render the "chat unavailable" fallback;
 *  - foreign hand-off: a tab inserted fail-open before ownership was known that
 *    the index now proves to belong to another project leaves the tree and is
 *    retained for its owner ONLY through `retainProjectChats(owner)` — never by
 *    writing another project's `editor-state` (that would race the owner's own
 *    debounced persist and could lose the tab);
 *  - an owner that is missing from the project list (or not loaded yet) or
 *    archived cannot restore the tab, so it stays where it is; it is ALSO
 *    retained for that owner (a deferred hand-off). The retained set outlives
 *    project switches, whereas the kept tab itself is dropped by the next
 *    switch (retainVisibleAgentChats / persistState exclude known-foreign
 *    chats and the layout reset discards the rest), so without the retain the
 *    open chat would be lost. When the owner becomes restorable its restore
 *    reattaches the tab.
 * Chat sessions keep running; no PTY is touched.
 */

import { chatForeignToProject, sessionOwnerProjectId } from '@/lib/acp-session-ownership'
import { logFrontendError } from '@/lib/log-api'
import { useAgentChatLifetimeStore } from '@/stores/agent-chat-lifetime-store'
import { useProjectStore } from '@/stores/project-store'
import { getAllLeafPanes, useWorkspaceStore } from '@/stores/workspace-store'

const SOURCE = 'acp-store.loadSessionIndex'

interface PruneRestoredChatTabsInput {
  liveSessionIds: ReadonlySet<string>
  indexedSessionIds: ReadonlySet<string>
  ownership: Parameters<typeof chatForeignToProject>[2]
  activeProjectId: string
}

function ownerCanRestore(owner: string): boolean {
  const projects = useProjectStore.getState().projects
  const project = projects.find((item) => item.id === owner)
  return project != null && !project.isArchived
}

export function pruneRestoredChatTabs(input: PruneRestoredChatTabsInput): void {
  const { liveSessionIds, indexedSessionIds, ownership, activeProjectId } = input
  const workspace = useWorkspaceStore.getState()
  for (const pane of getAllLeafPanes(workspace.root)) {
    for (const tab of pane.tabs) {
      if (tab.type !== 'agent-chat') continue
      if (!liveSessionIds.has(tab.sessionId) && !indexedSessionIds.has(tab.sessionId)) {
        workspace.removeTab(tab.id)
        continue
      }
      if (!chatForeignToProject(tab.sessionId, activeProjectId, ownership)) continue
      const owner = sessionOwnerProjectId(tab.sessionId, ownership)
      if (!owner) continue
      if (!ownerCanRestore(owner)) {
        useAgentChatLifetimeStore.getState().retainProjectChats(owner, [tab.sessionId])
        void logFrontendError({
          level: 'warn',
          source: SOURCE,
          message: `Kept foreign-project chat tab (session ${tab.sessionId}, owner ${owner}) in project ${activeProjectId}: owner project is missing or archived; hand-off deferred until it restores`
        })
        continue
      }
      useAgentChatLifetimeStore.getState().retainProjectChats(owner, [tab.sessionId])
      workspace.removeTab(tab.id)
      void logFrontendError({
        level: 'warn',
        source: SOURCE,
        message: `Removed foreign-project chat tab (session ${tab.sessionId}, owner ${owner}) from project ${activeProjectId}`
      })
    }
  }
}
