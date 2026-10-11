import type {
  GitCommitContext,
  GitDiffStat,
  GitStashInfo,
  GitStatusDetail
} from '@shared/types/ipc.types'
import { toast } from 'sonner'
import { create } from 'zustand'
import { gitApi } from '@/lib/git-api'
import { logFrontendError } from '@/lib/log-api'
import { platform } from '@/lib/tauri-os'
import { normalizePath } from '@/lib/working-copy'
import { useProjectStore } from './project-store'
import { useTerminalStore } from './terminal-store'

/**
 * Single git cache (shell revamp AD-17). Every per-working-copy map is keyed by
 * `normalizePath(path)` so `E:\p` and `e:/p/` share one entry; `paths[key]`
 * keeps the first display path seen (usable as a cwd) for refreshes. Callers
 * keep passing the raw `cwd` to actions and read through the same key.
 */

/** Build the staged-aware diff cache key so the staged and unstaged rows of the
 * same file (porcelain `MM`) do not collide. `cwd` is normalized. */
export const diffKey = (cwd: string, path: string, staged: boolean) =>
  `${normalizePath(cwd)}:${path}:${staged}`

/**
 * Changed files for `cwd`. A file that is both staged and unstaged has two
 * status entries but counts once, so every Git badge shows the same number.
 */
export const selectChangedFileCount =
  (cwd: string | null | undefined) =>
  (state: Pick<GitStatusState, 'statuses'>): number => {
    const statuses = cwd ? state.statuses[normalizePath(cwd)] : undefined
    if (!statuses || statuses.length === 0) return 0
    return new Set(statuses.map((status) => status.path)).size
  }

/** Current branch of the cached commit context (`null` on a detached HEAD or
 * when nothing is cached). There is no second branch cache. */
export const selectGitBranch =
  (cwd: string | null | undefined) =>
  (state: Pick<GitStatusState, 'commitContexts'>): string | null =>
    (cwd ? state.commitContexts[normalizePath(cwd)]?.branch : null) ?? null

export const selectGitDiffStat =
  (cwd: string | null | undefined) =>
  (state: Pick<GitStatusState, 'diffStats'>): GitDiffStat | null =>
    (cwd ? state.diffStats[normalizePath(cwd)] : null) ?? null

const NO_ERRORS: GitStatusErrors = Object.freeze({})

export const selectGitErrors =
  (cwd: string | null | undefined) =>
  (state: Pick<GitStatusState, 'errors'>): GitStatusErrors =>
    (cwd ? state.errors[normalizePath(cwd)] : undefined) ?? NO_ERRORS

/** The deepest tracked key containing `path` (segment-bounded prefix), or
 * `null` when no tracked working copy contains it. */
export function deepestTrackedKey(path: string, keys: Iterable<string>): string | null {
  const target = normalizePath(path)
  if (!target) return null
  let best: string | null = null
  for (const key of keys) {
    if (!key) continue
    const contains = target === key || target.startsWith(key.endsWith('/') ? key : `${key}/`)
    if (contains && (best === null || key.length > best.length)) best = key
  }
  return best
}

/** Failure messages per facade call; set on failure, cleared on success. */
export interface GitStatusErrors {
  status?: string
  commitContext?: string
  diffStat?: string
}

export interface GitStatusState {
  // Keys below are `normalizePath(cwd)`.
  // paths[key] = first display path seen for the key (usable as a cwd)
  paths: Record<string, string>
  // statuses[key] = GitStatusDetail[]
  statuses: Record<string, GitStatusDetail[]>
  // diffs["key:path:staged"] = string
  diffs: Record<string, string>
  // commitContexts[key] = GitCommitContext
  commitContexts: Record<string, GitCommitContext>
  // diffStats[key] = added/removed line totals (watched keys only)
  diffStats: Record<string, GitDiffStat>
  // errors[key] = last failure message per facade call
  errors: Record<string, GitStatusErrors>
  // stashes[key] = GitStashInfo[]
  stashes: Record<string, GitStashInfo[]>
  // branches[key] = string[]
  branches: Record<string, string[]>
  selectedFile: string | null
  isFetchingStatus: boolean
  statusFetchCount: number

  setSelectedFile: (path: string | null) => void
  refreshStatus: (cwd: string) => Promise<void>
  /** Fetches the diff stat of a watched key, at most once per 30 s; a throttled
   * request schedules one trailing refresh. No-op for unwatched keys. */
  refreshDiffStat: (cwd: string) => Promise<void>
  /** Ref-counted opt-in to diff-stat refreshes for `cwd`; returns the unwatch fn. */
  watchDiffStat: (cwd: string) => () => void
  /** Status + commit context + (if watched) diff stat for one working copy.
   * Never rejects; failures land in `errors`. */
  refreshWorkingCopy: (cwd: string) => Promise<void>
  /** `refreshWorkingCopy` for every tracked key. Never rejects. */
  refreshTracked: () => Promise<void>
  fetchDiff: (cwd: string, path: string, staged: boolean) => Promise<void>
  fetchCommitContext: (cwd: string) => Promise<GitCommitContext | null>
  fetchStashes: (cwd: string) => Promise<void>
  fetchBranches: (cwd: string) => Promise<void>
  stageFile: (cwd: string, path: string) => Promise<void>
  unstageFile: (cwd: string, path: string) => Promise<void>
  discardFile: (cwd: string, path: string) => Promise<void>
  stageFiles: (cwd: string, paths: string[]) => Promise<void>
  unstageFiles: (cwd: string, paths: string[]) => Promise<void>
  discardFiles: (cwd: string, paths: string[]) => Promise<void>
  stageHunk: (cwd: string, path: string, hunkPatch: string) => Promise<void>
  unstageHunk: (cwd: string, path: string, hunkPatch: string) => Promise<void>
  commit: (cwd: string, summary: string, description: string, amend: boolean) => Promise<void>
  push: (cwd: string) => Promise<void>
  stashSave: (cwd: string, message?: string, includeUntracked?: boolean) => Promise<void>
  stashApply: (cwd: string, index: number) => Promise<void>
  stashPop: (cwd: string, index: number) => Promise<void>
  stashDrop: (cwd: string, index: number) => Promise<void>
  branchSwitch: (cwd: string, name: string) => Promise<void>
  branchCreate: (cwd: string, name: string) => Promise<void>
}

export const useGitStatusStore = create<GitStatusState>((set, get) => ({
  paths: {},
  statuses: {},
  diffs: {},
  commitContexts: {},
  diffStats: {},
  errors: {},
  stashes: {},
  branches: {},
  selectedFile: null,
  isFetchingStatus: false,
  statusFetchCount: 0,

  setSelectedFile: (path) => set({ selectedFile: path }),

  refreshStatus: async (cwd) => {
    const key = trackPath(cwd)
    // Per-key request ordering: a slow earlier refresh (focus/poll started before
    // a git write) must not overwrite the newer status from the write's refresh.
    // biome-ignore lint/suspicious/noAssignInExpressions: deliberate request-token bump and capture
    const token = (statusRequests[key] = (statusRequests[key] ?? 0) + 1)
    const isCurrent = () => statusRequests[key] === token
    set((state) => ({
      statusFetchCount: state.statusFetchCount + 1,
      isFetchingStatus: true
    }))
    try {
      const status = await gitApi.getStatus(cwd)
      if (isCurrent()) {
        set((state) => ({
          statuses: { ...state.statuses, [key]: status }
        }))
        patchError(key, 'status', null)
      }
    } catch (error) {
      // Keep the previous status; record the failure and let the caller decide.
      if (isCurrent()) patchError(key, 'status', errorMessage(error))
      logFailure('status refresh', key, error)
      throw error
    } finally {
      set((state) => {
        const statusFetchCount = Math.max(0, state.statusFetchCount - 1)
        return {
          statusFetchCount,
          isFetchingStatus: statusFetchCount > 0
        }
      })
    }
  },

  refreshDiffStat: async (cwd) => {
    const key = normalizePath(cwd)
    if (!diffStatWatchers.has(key)) return
    const now = Date.now()
    const last = diffStatLastFetch.get(key)
    if (last !== undefined && now - last < DIFF_STAT_MIN_INTERVAL_MS) {
      // Throttled: coalesce into one trailing refresh at `last + 30 s`.
      if (!diffStatTrailing.has(key)) {
        diffStatTrailing.set(
          key,
          setTimeout(
            () => {
              diffStatTrailing.delete(key)
              void get().refreshDiffStat(cwd)
            },
            last + DIFF_STAT_MIN_INTERVAL_MS - now
          )
        )
      }
      return
    }
    diffStatLastFetch.set(key, now)
    trackPath(cwd)
    try {
      const stat = await gitApi.getDiffStat(cwd)
      set((state) => ({ diffStats: { ...state.diffStats, [key]: stat } }))
      patchError(key, 'diffStat', null)
    } catch (error) {
      patchError(key, 'diffStat', errorMessage(error))
      logFailure('diff stat refresh', key, error)
    }
  },

  watchDiffStat: (cwd) => {
    const key = trackPath(cwd)
    diffStatWatchers.set(key, (diffStatWatchers.get(key) ?? 0) + 1)
    let released = false
    return () => {
      if (released) return
      released = true
      const remaining = (diffStatWatchers.get(key) ?? 1) - 1
      if (remaining > 0) {
        diffStatWatchers.set(key, remaining)
        return
      }
      diffStatWatchers.delete(key)
      clearDiffStatTrailing(key)
    }
  },

  refreshWorkingCopy: async (cwd) => {
    await Promise.all([
      get()
        .refreshStatus(cwd)
        .catch(() => {
          // Stored in `errors` and logged by refreshStatus.
        }),
      get().fetchCommitContext(cwd),
      get().refreshDiffStat(cwd)
    ])
  },

  refreshTracked: async () => {
    await Promise.all(Object.values(get().paths).map((path) => get().refreshWorkingCopy(path)))
  },

  fetchCommitContext: async (cwd) => {
    const key = trackPath(cwd)
    // Guard against out-of-order responses: a slow earlier fetch must not
    // overwrite or delete a fresher result. Mirror the fetchDiff token pattern.
    // biome-ignore lint/suspicious/noAssignInExpressions: deliberate request-token bump and capture
    const token = (commitContextRequests[key] = (commitContextRequests[key] ?? 0) + 1)
    const isCurrent = () => commitContextRequests[key] === token
    try {
      const context = await gitApi.getCommitContext(cwd)
      if (isCurrent()) {
        set((state) => ({
          commitContexts: { ...state.commitContexts, [key]: context }
        }))
        patchError(key, 'commitContext', null)
        return context
      }
      // Superseded by a newer fetch: never hand the caller an older context.
      return null
    } catch (error) {
      if (!isCurrent()) return null
      // Drop any stale context so the footer disables its actions rather than
      // acting on out-of-date ahead/staged/HEAD data.
      set((state) => {
        if (!(key in state.commitContexts)) return {}
        const commitContexts = { ...state.commitContexts }
        delete commitContexts[key]
        return { commitContexts }
      })
      patchError(key, 'commitContext', errorMessage(error))
      logFailure('commit context fetch', key, error)
      return null
    }
  },

  fetchDiff: async (cwd, path, staged) => {
    const key = diffKey(cwd, path, staged)
    // Capture the request token before awaiting so a mutation that invalidates
    // this key while the fetch is in flight causes the late response to be
    // discarded instead of overwriting the cache with a stale diff.
    const token = diffRequestVersions[key] ?? 0
    const isCurrent = () => (diffRequestVersions[key] ?? 0) === token
    try {
      const diff = await gitApi.getDiff(cwd, path, staged)
      if (!isCurrent()) return
      set((state) => ({
        diffs: { ...state.diffs, [key]: diff }
      }))
    } catch (error) {
      console.error('Failed to fetch diff:', error)
      if (!isCurrent()) return
      // Write an empty sentinel so the panel stops retrying on every render and
      // shows the "no diff available" state instead of an infinite spinner.
      set((state) => ({
        diffs: { ...state.diffs, [key]: '' }
      }))
      toast.error(`Failed to load diff: ${String(error)}`)
    }
  },

  // Single-file mutations delegate to the batch variants so the post-mutation
  // refresh logic lives in exactly one place. The commit footer's staged count
  // is refreshed afterwards so Commit enables/disables correctly.
  stageFile: async (cwd, path) => get().stageFiles(cwd, [path]),

  unstageFile: async (cwd, path) => get().unstageFiles(cwd, [path]),

  discardFile: async (cwd, path) => get().discardFiles(cwd, [path]),

  // Batch mutations apply the per-file git operation to every path, then
  // refresh status + commit context once at the end rather than after each
  // file. The first failing path aborts the run (subsequent files are left
  // untouched) but a refresh still runs so the UI reflects what did change.
  stageFiles: async (cwd, paths) => {
    if (paths.length === 0) return
    try {
      for (const path of paths) {
        await gitApi.stage(cwd, path)
        invalidateFileDiffs(set, cwd, path)
      }
    } finally {
      await refreshAfterWrite(get, cwd)
    }
  },

  unstageFiles: async (cwd, paths) => {
    if (paths.length === 0) return
    try {
      for (const path of paths) {
        await gitApi.unstage(cwd, path)
        invalidateFileDiffs(set, cwd, path)
      }
    } finally {
      await refreshAfterWrite(get, cwd)
    }
  },

  // Per-hunk stage/unstage (#257). Same refresh contract as the file-level
  // mutations above: invalidate the cached diff for the file, then refresh
  // status + commit context so the panel reflects the partial stage.
  stageHunk: async (cwd, path, hunkPatch) => {
    try {
      await gitApi.stageHunk(cwd, path, hunkPatch)
      invalidateFileDiffs(set, cwd, path)
    } finally {
      await refreshAfterWrite(get, cwd)
    }
  },

  unstageHunk: async (cwd, path, hunkPatch) => {
    try {
      await gitApi.unstageHunk(cwd, path, hunkPatch)
      invalidateFileDiffs(set, cwd, path)
    } finally {
      await refreshAfterWrite(get, cwd)
    }
  },

  discardFiles: async (cwd, paths) => {
    if (paths.length === 0) return
    try {
      for (const path of paths) {
        await gitApi.discard(cwd, path)
        invalidateFileDiffs(set, cwd, path)
      }
    } finally {
      await refreshAfterWrite(get, cwd)
    }
  },

  commit: async (cwd, summary, description, amend) => {
    await gitApi.commit(cwd, summary, description, amend)
    // The mutation succeeded. Refresh status/context to reflect the new HEAD,
    // but never let a refresh failure surface as a commit failure (the commit
    // already happened) â€” swallow refresh errors here.
    await refreshAfterMutation(get, cwd)
  },

  push: async (cwd) => {
    await gitApi.push(cwd)
    await refreshAfterMutation(get, cwd)
  },

  fetchStashes: async (cwd) => {
    const key = trackPath(cwd)
    try {
      const stashes = await gitApi.stashList(cwd)
      set((state) => ({
        stashes: { ...state.stashes, [key]: stashes }
      }))
    } catch (error) {
      console.error('Failed to fetch stashes:', error)
    }
  },

  fetchBranches: async (cwd) => {
    const key = trackPath(cwd)
    try {
      const branches = await gitApi.branchList(cwd)
      set((state) => ({
        branches: { ...state.branches, [key]: branches }
      }))
    } catch (error) {
      console.error('Failed to fetch branches:', error)
    }
  },

  stashSave: async (cwd, message, includeUntracked) => {
    await gitApi.stashSave(cwd, message, includeUntracked)
    await refreshAfterMutation(get, cwd)
  },

  stashApply: async (cwd, index) => {
    await gitApi.stashApply(cwd, index)
    await refreshAfterMutation(get, cwd)
  },

  stashPop: async (cwd, index) => {
    await gitApi.stashPop(cwd, index)
    await refreshAfterMutation(get, cwd)
  },

  stashDrop: async (cwd, index) => {
    await gitApi.stashDrop(cwd, index)
    await get().fetchStashes(cwd)
  },

  branchSwitch: async (cwd, name) => {
    await gitApi.branchSwitch(cwd, name)
    set({ selectedFile: null })
    await refreshAfterMutation(get, cwd)
    await updateStoresWithBranch(cwd, name)
  },

  branchCreate: async (cwd, name) => {
    await gitApi.branchCreate(cwd, name)
    set({ selectedFile: null })
    await refreshAfterMutation(get, cwd)
    await updateStoresWithBranch(cwd, name)
  }
}))

async function updateStoresWithBranch(cwd: string, branchName: string) {
  try {
    const slashPath = (p?: string) => (p ? p.replace(/\\/g, '/') : '')
    const normalizedCwd = slashPath(cwd)

    const isWindows = platform() === 'windows'
    const matchPath = (otherPath?: string) => {
      const normalizedOther = slashPath(otherPath)
      return isWindows
        ? normalizedOther.toLowerCase() === normalizedCwd.toLowerCase()
        : normalizedOther === normalizedCwd
    }

    const projectStore = useProjectStore.getState()
    const project = projectStore.projects.find((p) => matchPath(p.path))
    if (project) {
      projectStore.updateProject(project.id, { gitBranch: branchName })
    }

    const terminalStore = useTerminalStore.getState()
    for (const t of terminalStore.terminals) {
      if (matchPath(t.cwd)) {
        terminalStore.updateTerminalGitBranch(t.id, branchName)
      }
    }
  } catch (err) {
    console.error('Failed to sync branch name across stores:', err)
  }
}

/** Refresh status, commit context and (if watched) diff stat after an own write.
 * Never rejects: refresh failures are stored in `errors` (and logged), so a
 * transient read failure neither masks the write's own error nor gets reported
 * as a failed commit/push that actually succeeded. */
async function refreshAfterWrite(get: () => GitStatusState, cwd: string): Promise<void> {
  try {
    await get().refreshStatus(cwd)
  } catch {
    // Stored in `errors` and logged by refreshStatus.
  }
  await get().fetchCommitContext(cwd)
  await get().refreshDiffStat(cwd)
}

/** `refreshAfterWrite` plus stash and branch lists, for writes that move them. */
async function refreshAfterMutation(get: () => GitStatusState, cwd: string): Promise<void> {
  await refreshAfterWrite(get, cwd)
  await get()
    .fetchStashes(cwd)
    .catch(() => {})
  await get()
    .fetchBranches(cwd)
    .catch(() => {})
}

/** Monotonic request token per diff key. Bumped whenever a key is invalidated
 * so an in-flight `fetchDiff` can detect it has been superseded. Kept outside
 * React state because it is control metadata, not render data. */
const diffRequestVersions: Record<string, number> = {}

/** Monotonic request token per key for `fetchCommitContext`. Lets a late
 * response detect it has been superseded by a newer fetch, so out-of-order
 * responses cannot overwrite or delete fresher context. Control metadata, not
 * render data, so it lives outside React state (same rationale as above). */
const commitContextRequests: Record<string, number> = {}

/** Monotonic request token per working-copy key for efreshStatus (same idea). */
const statusRequests: Record<string, number> = {}

const DIFF_STAT_MIN_INTERVAL_MS = 30_000

/** Watcher ref-counts, last fetch time and the pending trailing refresh per key
 * for `refreshDiffStat`. Control metadata, kept outside React state. */
const diffStatWatchers = new Map<string, number>()
const diffStatLastFetch = new Map<string, number>()
const diffStatTrailing = new Map<string, ReturnType<typeof setTimeout>>()

function clearDiffStatTrailing(key: string) {
  const timer = diffStatTrailing.get(key)
  if (timer === undefined) return
  clearTimeout(timer)
  diffStatTrailing.delete(key)
}

/** Test-only: drop watcher/throttle/token metadata and any pending timer. */
export function resetGitStatusStoreControlForTests(): void {
  for (const key of [...diffStatTrailing.keys()]) clearDiffStatTrailing(key)
  diffStatWatchers.clear()
  diffStatLastFetch.clear()
  for (const key of Object.keys(commitContextRequests)) delete commitContextRequests[key]
  for (const key of Object.keys(statusRequests)) delete statusRequests[key]
}

/** Canonical key for `cwd`; records the first display path seen for it so
 * refreshes have a usable cwd. */
function trackPath(cwd: string): string {
  const key = normalizePath(cwd)
  if (useGitStatusStore.getState().paths[key] === undefined) {
    useGitStatusStore.setState((state) =>
      state.paths[key] === undefined ? { paths: { ...state.paths, [key]: cwd } } : {}
    )
  }
  return key
}

/** Set (`message`) or clear (`null`) one failure slot of `errors[key]`. */
function patchError(key: string, field: keyof GitStatusErrors, message: string | null) {
  const current = useGitStatusStore.getState().errors[key]
  if (message === null ? current?.[field] === undefined : current?.[field] === message) return
  useGitStatusStore.setState((state) => {
    const next = { ...state.errors[key] }
    if (message === null) delete next[field]
    else next[field] = message
    const errors = { ...state.errors }
    if (Object.keys(next).length === 0) delete errors[key]
    else errors[key] = next
    return { errors }
  })
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/** One backend record per failed outcome. Never include branch names. */
function logFailure(outcome: string, key: string, error: unknown) {
  void logFrontendError({
    level: 'warn',
    message: `[git-status] ${outcome} failed for ${key}: ${errorMessage(error)}`,
    source: 'git-status-store'
  })
}

function bumpDiffVersion(key: string) {
  diffRequestVersions[key] = (diffRequestVersions[key] ?? 0) + 1
}

/** Drop cached staged and unstaged diffs for a file after it mutates so the
 * panel refetches the now-current diff instead of showing a stale one. Also
 * bumps each key's request token to discard any in-flight fetch. */
function invalidateFileDiffs(
  set: (fn: (state: GitStatusState) => Partial<GitStatusState>) => void,
  cwd: string,
  path: string
) {
  const stagedKey = diffKey(cwd, path, true)
  const unstagedKey = diffKey(cwd, path, false)
  bumpDiffVersion(stagedKey)
  bumpDiffVersion(unstagedKey)
  set((state) => {
    const diffs = { ...state.diffs }
    delete diffs[stagedKey]
    delete diffs[unstagedKey]
    return { diffs }
  })
}
