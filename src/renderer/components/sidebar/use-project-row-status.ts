import { useCallback } from 'react'
import { useAgentChatProjectSignals } from '@/hooks/use-agent-chat-attention'
import { useProjectsWithActiveAgentChat } from '@/stores/acp-store'
import { useProjectsWithActivity, useProjectsWithErrors } from '@/stores/terminal-store'
import { type ProjectRowStatus, resolveProjectLiveState } from './indicators'

/**
 * Status of each project for the Rail (expanded rows and collapsed icons):
 * live work, agent chats that need you, and a crashed terminal. Subscribes
 * only to which projects have activity, not to terminal output.
 */
export function useProjectRowStatus(): (projectId: string) => ProjectRowStatus {
  const [projectActivityIds, projectErrorIds] = [useProjectsWithActivity(), useProjectsWithErrors()]
  const agentChatActivityIds = useProjectsWithActiveAgentChat()
  const { attentionCounts, runningProjectIds } = useAgentChatProjectSignals()

  return useCallback(
    (projectId: string): ProjectRowStatus => ({
      live: resolveProjectLiveState(
        projectActivityIds.includes(projectId) || agentChatActivityIds.includes(projectId),
        runningProjectIds.has(projectId)
      ),
      attentionCount: attentionCounts[projectId] ?? 0,
      crashed: projectErrorIds.has(projectId)
    }),
    [projectActivityIds, agentChatActivityIds, runningProjectIds, attentionCounts, projectErrorIds]
  )
}
