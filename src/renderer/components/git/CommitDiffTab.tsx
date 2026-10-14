import { useCallback, useEffect, useRef, useState } from 'react'
import { GitDiffPane } from '@/components/git/GitDiffPane'
import { gitApi } from '@/lib/git-api'
import { logFrontendError } from '@/lib/log-api'
import { type GitDiffViewMode, loadGitDiffViewMode } from '@/lib/parse-unified-diff'

interface CommitDiffTabProps {
  cwd: string
  /** Full commit hash. */
  commit: string
  /** Repo-relative path of the new side. */
  path: string
  /** A renamed file's path before the commit. */
  oldPath?: string
  isVisible: boolean
}

type LoadState = { diff: string | null; error: string | null }

const NO_HUNK_ACTION = async (): Promise<void> => {}

/**
 * Main-area read-only Diff tab for one file at one commit (Git History). The
 * diff is against the commit's first parent. A commit does not change, so the
 * tab loads once on first show and keeps the result in local state.
 */
export function CommitDiffTab({
  cwd,
  commit,
  path,
  oldPath,
  isVisible
}: CommitDiffTabProps): React.JSX.Element {
  const [state, setState] = useState<LoadState>({ diff: null, error: null })
  const [diffViewMode, setDiffViewMode] = useState<GitDiffViewMode>(loadGitDiffViewMode)
  const requested = useRef(false)
  const mounted = useRef(true)

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])

  const load = useCallback((): void => {
    requested.current = true
    setState({ diff: null, error: null })
    gitApi
      .getCommitFileDiff(cwd, commit, path, oldPath)
      .then((diff) => {
        if (mounted.current) setState({ diff, error: null })
      })
      .catch((cause: unknown) => {
        const message = cause instanceof Error ? cause.message : String(cause)
        void logFrontendError({
          level: 'warn',
          source: 'CommitDiffTab.load',
          message: `commit file diff failed for ${commit.slice(0, 12)}: ${message}`
        })
        if (mounted.current) setState({ diff: null, error: message })
      })
  }, [cwd, commit, path, oldPath])

  useEffect(() => {
    if (isVisible && !requested.current) load()
  }, [isVisible, load])

  return (
    <div className="flex h-full w-full flex-col min-w-0 bg-card/30">
      <GitDiffPane
        variant="desktop"
        selectedFile={path}
        selectedStaged={false}
        sideLabel={`at ${commit.slice(0, 7)}`}
        currentDiff={state.error === null ? state.diff : undefined}
        diffViewMode={diffViewMode}
        onDiffViewModeChange={setDiffViewMode}
        onStageHunk={NO_HUNK_ACTION}
        onUnstageHunk={NO_HUNK_ACTION}
        failure={state.error === null ? undefined : { detail: state.error, onRetry: load }}
      />
    </div>
  )
}
