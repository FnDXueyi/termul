import type { GitCommit } from '@shared/types/ipc.types'
import type React from 'react'
import { useEffect, useMemo, useState } from 'react'
import { History, RefreshCw, Search } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { PANEL_FIELD_CLASS, PANEL_FIELD_ICON_CLASS } from '@/components/ui/panel-styles'
import { Spinner } from '@/components/ui/spinner'
import { useMobileWebShell } from '@/hooks/use-mobile-web-shell'
import { computeGraphLayout, type GraphLayout } from '@/lib/git-graph-layout'
import { formatRelativeTime } from '@/lib/git-time'
import { cn } from '@/lib/utils'
import { useGitHistoryStore } from '@/stores/git-history-store'
import { GitHistoryView } from './GitHistoryView'
import { matchesCommitQuery } from './history/commit-filter'
import {
  graphColumnWidth,
  LaneGraph,
  MOBILE_GRAPH_GEOMETRY,
  RefChip,
  rowIndexByHash
} from './history/lane-graph'

interface GitHistoryPanelProps {
  cwd: string
  isVisible: boolean
}

/** The git-history tab: the main-panel {@link GitHistoryView} on desktop, a
 * two-line touch list on the mobile web shell. */
export function GitHistoryPanel({ cwd, isVisible }: GitHistoryPanelProps): React.JSX.Element {
  const isMobileWebShell = useMobileWebShell()
  if (!isMobileWebShell) return <GitHistoryView cwd={cwd} isVisible={isVisible} />
  return <MobileGitHistory cwd={cwd} isVisible={isVisible} />
}

function MobileGitHistory({ cwd, isVisible }: GitHistoryPanelProps): React.JSX.Element {
  const commits = useGitHistoryStore((state) => state.commits[cwd])
  const isLoading = useGitHistoryStore((state) => state.loading[cwd] ?? false)
  const error = useGitHistoryStore((state) => state.error[cwd] ?? null)
  const refreshLog = useGitHistoryStore((state) => state.refreshLog)

  const [searchQuery, setSearchQuery] = useState('')

  useEffect(() => {
    // Fetch on first reveal (or when no data yet) and on cwd change.
    if (isVisible && commits === undefined) {
      void refreshLog(cwd)
    }
  }, [isVisible, cwd, commits, refreshLog])

  const filteredCommits = useMemo(() => {
    const list = commits ?? []
    if (!searchQuery.trim()) return list
    const q = searchQuery.toLowerCase()
    return list.filter((c) => matchesCommitQuery(c, q))
  }, [commits, searchQuery])

  // The lane graph reflects true topology, so it is computed from the full
  // commit list, not the filtered view. Filtering only affects the row list.
  const layout: GraphLayout = useMemo(() => computeGraphLayout(commits ?? []), [commits])
  const rowByHash = useMemo(() => rowIndexByHash(layout), [layout])

  const geometry = MOBILE_GRAPH_GEOMETRY
  const graphWidth = graphColumnWidth(geometry, layout.laneCount)
  const graphHeight = Math.max(1, layout.rows.length) * geometry.rowHeight
  const isFiltering = searchQuery.trim().length > 0

  // Phone: two-line 44px rows, a full-width 16px filter field on its own row,
  // a 44px refresh and a plain scroll container.
  return (
    <div className="flex h-full w-full flex-col bg-background overflow-hidden">
      <div className="p-3 border-b border-border flex flex-col gap-1 shrink-0">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 text-sm font-medium text-foreground">
            <History size={15} className="text-primary" />
            Git History
          </div>
          <Button
            variant="ghost"
            size="touch"
            className="w-11"
            onClick={() => refreshLog(cwd)}
            disabled={isLoading}
            title="Refresh history"
            aria-label="Refresh history"
          >
            {isLoading ? <Spinner size={16} decorative /> : <RefreshCw className="h-4 w-4" />}
          </Button>
        </div>
        <div className="relative">
          <Search size={14} aria-hidden className={PANEL_FIELD_ICON_CLASS} />
          <input
            type="text"
            placeholder="Filter commits..."
            aria-label="Filter commits"
            className={cn(PANEL_FIELD_CLASS, 'h-11 w-full pl-9 pr-3 text-base')}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>
      </div>

      {commits === undefined && isLoading ? (
        <LoadingState />
      ) : (commits?.length ?? 0) === 0 ? (
        <EmptyState error={error} />
      ) : (
        <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain">
          <div className="relative" style={{ minHeight: isFiltering ? undefined : graphHeight }}>
            {!isFiltering && (
              <LaneGraph
                layout={layout}
                rowByHash={rowByHash}
                geometry={geometry}
                width={graphWidth}
                height={graphHeight}
              />
            )}

            {/* Unfiltered rows clear the graph column; while filtering the
               graph is hidden, so rows run full width with a left inset. */}
            <div
              className={isFiltering ? 'pl-3' : undefined}
              style={{ paddingLeft: isFiltering ? undefined : graphWidth }}
            >
              {isFiltering && filteredCommits.length === 0 ? (
                <div className="break-words px-4 py-8 text-center text-sm text-muted-foreground">
                  No commits match "{searchQuery}"
                </div>
              ) : (
                (isFiltering ? filteredCommits : layout.rows.map((r) => r.commit)).map((commit) => (
                  <MobileCommitRow
                    key={commit.hash}
                    commit={commit}
                    rowHeight={geometry.rowHeight}
                  />
                ))
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function LoadingState(): React.JSX.Element {
  return (
    <div className="flex-1 flex items-center justify-center text-muted-foreground">
      <Spinner size={16} decorative className="mr-2" />
      Loading history...
    </div>
  )
}

/** Phone row: subject over a meta line (refs, author, time, short hash), at the
 * 44px touch pitch the lane graph aligns to. Every line is single-line; time and
 * hash never shrink, the subject and author truncate, refs clip at half the line
 * and, on a very narrow line, shrink before the time or hash is ever cut. */
function MobileCommitRow({
  commit,
  rowHeight
}: {
  commit: GitCommit
  rowHeight: number
}): React.JSX.Element {
  return (
    <div
      className="flex flex-col justify-center overflow-hidden pr-3 border-b border-border/40"
      style={{ height: rowHeight }}
      title={`${commit.shortHash} — ${commit.subject}`}
    >
      {/* min-h-5 keeps the line's 20px when the subject is empty (an
         --allow-empty-message commit), so the meta line stays put. */}
      <div className="min-h-5 min-w-0 truncate text-sm leading-5 text-foreground">
        {commit.subject}
      </div>
      <div className="flex min-w-0 items-center gap-2 text-xs leading-4 text-muted-foreground">
        {commit.refs.length > 0 && (
          <div className="flex min-w-0 max-w-[50%] items-center gap-1.5 overflow-hidden whitespace-nowrap [&>*]:shrink-0">
            {commit.refs.map((ref) => (
              <RefChip key={ref} raw={ref} />
            ))}
          </div>
        )}
        <span className="min-w-0 flex-1 truncate">{commit.author}</span>
        <span className="shrink-0 tabular-nums">{formatRelativeTime(commit.date)}</span>
        <span className="shrink-0 font-mono">{commit.shortHash}</span>
      </div>
    </div>
  )
}

function EmptyState({ error }: { error: string | null }): React.JSX.Element {
  return (
    <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground p-8 text-center">
      <div className="w-12 h-12 rounded-full bg-secondary flex items-center justify-center mb-4 text-muted-foreground/50">
        <History size={24} />
      </div>
      <h3 className="text-sm font-medium text-foreground mb-1">No commit history</h3>
      <p className="text-xs max-w-[260px]">
        {error
          ? 'This folder may not be a Git repository, or git is unavailable.'
          : 'There are no commits to show yet. Make your first commit to see it here.'}
      </p>
    </div>
  )
}
