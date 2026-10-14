import type React from 'react'
import { useEffect } from 'react'
import { Check, ExternalLink, History } from '@/components/icons'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { formatRelativeTime } from '@/lib/git-time'
import { cn } from '@/lib/utils'
import { useGitHistoryStore } from '@/stores/git-history-store'
import { findPaneContainingTab, gitHistoryTabId, useWorkspaceStore } from '@/stores/workspace-store'

interface HistoryRowProps {
  cwd: string
  isVisible: boolean
}

/**
 * Bottom row of the shell Changes panel: the last commit and the commit count.
 * A click opens (or focuses) the Git History tab in the main panel; a check
 * shows while that tab is open.
 */
export function HistoryRow({ cwd, isVisible }: HistoryRowProps): React.JSX.Element {
  const commits = useGitHistoryStore((state) => state.commits[cwd])
  const isLoading = useGitHistoryStore((state) => state.loading[cwd] ?? false)
  const refreshLog = useGitHistoryStore((state) => state.refreshLog)
  const addGitHistoryTab = useWorkspaceStore((state) => state.addGitHistoryTab)
  const isOpen = useWorkspaceStore(
    (state) => findPaneContainingTab(state.root, gitHistoryTabId(cwd)) !== null
  )

  useEffect(() => {
    if (isVisible && commits === undefined && !isLoading) void refreshLog(cwd)
  }, [isVisible, cwd, commits, isLoading, refreshLog])

  const last = commits?.[0]
  const count = commits?.length ?? 0
  const countLabel =
    commits === undefined
      ? ''
      : `${count}${count >= 200 ? '+' : ''} ${count === 1 ? 'commit' : 'commits'}`

  return (
    <div className="shrink-0 border-t border-border p-2">
      <button
        type="button"
        onClick={() => addGitHistoryTab(cwd)}
        aria-label={isOpen ? 'History, open in the main panel' : 'Open History in the main panel'}
        className={cn(
          'flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors duration-150 ease-out hover:bg-foreground/[0.03]',
          FOCUS_RING_CLASS
        )}
      >
        <History size={15} className="shrink-0 text-muted-foreground" />
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex items-center gap-1.5">
            <span className="text-xs text-foreground">History</span>
            <span className="flex-1" />
            <span className="shrink-0 text-2xs tabular-nums text-muted-foreground">
              {countLabel}
            </span>
          </span>
          <span className="truncate text-2xs text-muted-foreground">
            {last
              ? `${last.subject || '(no message)'} · ${formatRelativeTime(last.date)}`
              : commits === undefined
                ? 'Loading...'
                : 'No commits yet'}
          </span>
        </span>
        {isOpen ? (
          <Check size={14} className="shrink-0 text-foreground" />
        ) : (
          <ExternalLink size={14} className="shrink-0 text-muted-foreground" />
        )}
      </button>
    </div>
  )
}
