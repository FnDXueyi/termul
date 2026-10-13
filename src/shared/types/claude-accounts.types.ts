/**
 * Claude Accounts contract (ADR-0004). Mirrors the Rust
 * `acp::claude_accounts` serde shapes. No field holds a secret: logins stay in
 * Claude Code's own credential store on the host.
 */

/** Id of the built-in Account that uses the host's shared Claude config folder. */
export const BUILTIN_CLAUDE_ACCOUNT_ID = 'default'

/** One Claude subscription login. */
export interface ClaudeAccount {
  id: string
  name: string
  /** Unix epoch milliseconds; `0` for the built-in Account. */
  createdAt: number
}

export interface ClaudeAccountView {
  id: string
  name: string
  /** `true` for the Account that uses the host's shared Claude config folder. */
  builtin: boolean
  /** Claude CLI login state; `null` when the host could not read it. */
  signedIn: boolean | null
}

export interface ClaudeAccountsSnapshot {
  accounts: ClaudeAccountView[]
  defaultAccountId: string
  /** Project id → Account id, only for Projects with an explicit choice. */
  projectAccounts: Record<string, string>
}
