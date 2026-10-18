import type {
  ClaudeAccountUsage,
  ClaudeAccountView,
  ClaudeUsageWindow
} from '@shared/types/claude-accounts.types'
import { accountReadiness } from '@/components/settings/provider/account-status'
import { type ClaudeAccountsState, useClaudeAccounts } from '@/hooks/use-claude-accounts'
import { primaryUsageWindow } from '@/lib/claude-usage-format'

export interface AccountChoice {
  account: ClaudeAccountView
  usage: ClaudeAccountUsage | undefined
  window: ClaudeUsageWindow | null
}

export interface ProjectClaudeAccount {
  accounts: ClaudeAccountsState
  projectId: string
  /** The Account the Project uses now. */
  current: AccountChoice
  defaultAccount: ClaudeAccountView | null
  /** `true` when the Project chose an Account (not "Follow default"). */
  explicit: boolean
  /** Signed-in Accounts with room left, least used first. */
  alternatives: AccountChoice[]
}

const choice = (
  account: ClaudeAccountView,
  usage: Record<string, ClaudeAccountUsage>
): AccountChoice => ({
  account,
  usage: usage[account.id],
  window: primaryUsageWindow(usage[account.id])
})

/**
 * The Claude Account context of one Project, before or without a chat.
 * `null` for no Project (nothing is read) and while the Account list loads.
 */
export function useProjectClaudeAccount(projectId: string | null): ProjectClaudeAccount | null {
  const accounts = useClaudeAccounts({ enabled: Boolean(projectId) })
  const { snapshot, usage } = accounts
  if (!projectId || !snapshot) return null

  const defaultAccount = snapshot.accounts.find((a) => a.id === snapshot.defaultAccountId) ?? null
  const chosenId = snapshot.projectAccounts[projectId]
  const currentAccount = snapshot.accounts.find(
    (a) => a.id === (chosenId ?? snapshot.defaultAccountId)
  )
  if (!currentAccount) return null

  const alternatives = snapshot.accounts
    .filter((a) => a.id !== currentAccount.id && accountReadiness(a, usage[a.id]) === 'ready')
    .map((a) => choice(a, usage))
    .filter((c) => (c.window?.percent ?? 0) < 100)
    .sort((x, y) => (x.window?.percent ?? 0) - (y.window?.percent ?? 0))

  return {
    accounts,
    projectId,
    current: choice(currentAccount, usage),
    defaultAccount,
    explicit: Boolean(chosenId),
    alternatives
  }
}
