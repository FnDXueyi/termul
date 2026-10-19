import type { ClaudeRateLimit } from '@shared/types/claude-accounts.types'
import { useAcpStore } from '@/stores/acp-store'
import { isClaudeSession } from '@/stores/claude-account-switch'
import { type ProjectClaudeAccount, useProjectClaudeAccount } from './use-project-claude-account'

export interface ChatClaudeAccount extends ProjectClaudeAccount {
  /** Live limit state from the running turn, when Claude sent one. */
  rateLimit: ClaudeRateLimit | null
}

/** The Claude Account context of one chat; `null` for a chat that is not a managed Claude chat. */
export function useChatClaudeAccount(sessionId: string): ChatClaudeAccount | null {
  const isClaude = useAcpStore((s) => isClaudeSession(s, sessionId))
  const projectId = useAcpStore((s) => s.sessions?.[sessionId]?.projectId ?? '')
  const rateLimit = useAcpStore((s) => s.sessionUsage?.[sessionId]?.rateLimit ?? null)
  const project = useProjectClaudeAccount(isClaude && projectId ? projectId : null)
  return project && { ...project, rateLimit }
}
