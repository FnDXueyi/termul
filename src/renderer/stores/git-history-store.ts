import type { GitCommit, GitCommitDetail } from '@shared/types/ipc.types'
import { toast } from 'sonner'
import { create } from 'zustand'
import { gitApi } from '@/lib/git-api'
import { logFrontendError } from '@/lib/log-api'

/** Which refs the history walks: every branch, tag and remote, or `HEAD` only. */
export type GitHistoryScope = 'all' | 'head'

/** One commit's detail request, keyed by working copy and full hash. */
export interface CommitDetailEntry {
  detail: GitCommitDetail | null
  loading: boolean
  error: string | null
}

export interface GitHistoryState {
  // commits[cwd] = GitCommit[] (newest first)
  commits: Record<string, GitCommit[]>
  // loading[cwd] = boolean
  loading: Record<string, boolean>
  // error[cwd] = string | null
  error: Record<string, string | null>
  // scope[cwd]; absent = 'all'
  scope: Record<string, GitHistoryScope>
  // details[commitDetailKey(cwd, hash)]
  details: Record<string, CommitDetailEntry>

  refreshLog: (cwd: string, limit?: number) => Promise<void>
  /** Change the refs the history walks, then reload it. */
  setScope: (cwd: string, scope: GitHistoryScope) => void
  /** Load one commit's detail once; a loaded or in-flight entry is reused. */
  loadCommitDetail: (cwd: string, hash: string) => Promise<void>
}

export function commitDetailKey(cwd: string, hash: string): string {
  return `${cwd}\u0000${hash}`
}

export const useGitHistoryStore = create<GitHistoryState>((set, get) => ({
  commits: {},
  loading: {},
  error: {},
  scope: {},
  details: {},

  refreshLog: async (cwd, limit) => {
    // Capture a per-cwd request token before awaiting so a later refresh that
    // resolves first cannot be overwritten by an earlier, slower response.
    const token = (logRequestVersions[cwd] ?? 0) + 1
    logRequestVersions[cwd] = token
    const isCurrent = () => logRequestVersions[cwd] === token
    const all = (get().scope[cwd] ?? 'all') === 'all'

    set((state) => ({
      loading: { ...state.loading, [cwd]: true },
      error: { ...state.error, [cwd]: null }
    }))

    try {
      const commits = await gitApi.getLog(cwd, limit, all)
      if (!isCurrent()) return
      set((state) => ({
        commits: { ...state.commits, [cwd]: commits },
        loading: { ...state.loading, [cwd]: false }
      }))
    } catch (err) {
      if (!isCurrent()) return
      const message = String(err)
      set((state) => ({
        // Keep any previously-loaded commits so a transient refresh failure
        // does not wipe the displayed history; only surface the error.
        loading: { ...state.loading, [cwd]: false },
        error: { ...state.error, [cwd]: message }
      }))
      toast.error(`Failed to load git history: ${message}`)
    }
  },

  setScope: (cwd, scope) => {
    if ((get().scope[cwd] ?? 'all') === scope) return
    set((state) => ({ scope: { ...state.scope, [cwd]: scope } }))
    void get().refreshLog(cwd)
  },

  loadCommitDetail: async (cwd, hash) => {
    const key = commitDetailKey(cwd, hash)
    const existing = get().details[key]
    if (existing && (existing.loading || existing.detail)) return

    set((state) => ({
      details: { ...state.details, [key]: { detail: null, loading: true, error: null } }
    }))
    try {
      const detail = await gitApi.getCommitDetail(cwd, hash)
      set((state) => ({
        details: { ...state.details, [key]: { detail, loading: false, error: null } }
      }))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      void logFrontendError({
        level: 'warn',
        source: 'git-history',
        message: `commit detail failed for ${hash.slice(0, 12)}: ${message}`
      })
      set((state) => ({
        details: { ...state.details, [key]: { detail: null, loading: false, error: message } }
      }))
    }
  }
}))

/** Monotonic request token per cwd, kept outside React state because it is
 * control metadata for discarding superseded in-flight fetches. */
const logRequestVersions: Record<string, number> = {}
