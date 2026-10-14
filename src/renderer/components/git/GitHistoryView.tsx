import type { GitCommit, GitCommitFile } from '@shared/types/ipc.types'
import type React from 'react'
import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react'
import { toast } from 'sonner'
import { History } from '@/components/icons'
import { Spinner } from '@/components/ui/spinner'
import { clipboardApi } from '@/lib/clipboard-api'
import { computeGraphLayout, type GraphLayout } from '@/lib/git-graph-layout'
import { formatRelativeTime } from '@/lib/git-time'
import { logFrontendError } from '@/lib/log-api'
import { cn } from '@/lib/utils'
import { commitDetailKey, useGitHistoryStore } from '@/stores/git-history-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import { AuthorAvatar } from './history/author-avatar'
import { CommitDetailPane } from './history/commit-detail-pane'
import { matchesCommitQuery } from './history/commit-filter'
import { HistoryToolbar } from './history/history-toolbar'
import {
  DESKTOP_GRAPH_GEOMETRY,
  graphColumnWidth,
  LaneGraph,
  RefChip,
  rowIndexByHash
} from './history/lane-graph'
import {
  DETAIL_PANE_MAX_WIDTH,
  DETAIL_PANE_MIN_WIDTH,
  useDetailPaneResize
} from './history/use-detail-pane-resize'

interface GitHistoryViewProps {
  cwd: string
  isVisible: boolean
}

const HEADER_HEIGHT = 28
/** Under this view width the detail pane floats over the table. */
const OVERLAY_BELOW_WIDTH = 880
/** Inset of the rows while filtering, when the graph column is hidden. */
const FILTERED_INSET = 12

function isTypingTarget(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)
  )
}

/** Authors by commit count, most active first. */
function authorsOf(commits: GitCommit[]): string[] {
  const counts = new Map<string, number>()
  for (const c of commits) counts.set(c.author, (counts.get(c.author) ?? 0) + 1)
  return [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([name]) => name)
}

/**
 * Desktop Git History tab in the main panel: toolbar, a commit table with the
 * lane graph, and a resizable commit detail pane. Keyboard: ↑/↓ select, Enter
 * opens the first changed file, Ctrl/Cmd+C copies the full hash, Esc hides
 * the details, `/` focuses the filter.
 */
export function GitHistoryView({ cwd, isVisible }: GitHistoryViewProps): React.JSX.Element {
  const commits = useGitHistoryStore((state) => state.commits[cwd])
  const isLoading = useGitHistoryStore((state) => state.loading[cwd] ?? false)
  const error = useGitHistoryStore((state) => state.error[cwd] ?? null)
  const scope = useGitHistoryStore((state) => state.scope[cwd] ?? 'all')
  const refreshLog = useGitHistoryStore((state) => state.refreshLog)
  const setScope = useGitHistoryStore((state) => state.setScope)
  const addCommitDiffTab = useWorkspaceStore((state) => state.addCommitDiffTab)

  const [query, setQuery] = useState('')
  const [author, setAuthor] = useState<string | null>(null)
  const [selectedHash, setSelectedHash] = useState<string | null>(null)
  const [detailOpen, setDetailOpen] = useState(true)
  const [viewWidth, setViewWidth] = useState(0)
  const rootRef = useRef<HTMLDivElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const filterRef = useRef<HTMLInputElement>(null)
  const idPrefix = useId()
  const pane = useDetailPaneResize()

  useEffect(() => {
    if (isVisible && commits === undefined) void refreshLog(cwd)
  }, [isVisible, cwd, commits, refreshLog])

  useEffect(() => {
    const node = rootRef.current
    if (!node || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(([entry]) => setViewWidth(entry.contentRect.width))
    observer.observe(node)
    return () => observer.disconnect()
  }, [])

  const all = commits ?? []
  const authors = useMemo(() => authorsOf(all), [all])
  // The lane graph reflects true topology, so it is computed from the full
  // commit list; a filtered subset hides it.
  const layout: GraphLayout = useMemo(() => computeGraphLayout(all), [all])
  const rowByHash = useMemo(() => rowIndexByHash(layout), [layout])
  const q = query.trim().toLowerCase()
  const isFiltering = q.length > 0 || author !== null
  const visible = useMemo(
    () =>
      isFiltering
        ? all.filter((c) => (!author || c.author === author) && (!q || matchesCommitQuery(c, q)))
        : layout.rows.map((r) => r.commit),
    [all, layout, isFiltering, author, q]
  )

  // Keep a selection: the first commit on load, and again when the selected
  // commit leaves the list (refresh, scope change).
  useEffect(() => {
    if (all.length === 0) return
    if (!selectedHash || !all.some((c) => c.hash === selectedHash)) setSelectedHash(all[0].hash)
  }, [all, selectedHash])

  const selected = all.find((c) => c.hash === selectedHash) ?? null
  const geometry = DESKTOP_GRAPH_GEOMETRY
  const graphWidth = isFiltering ? FILTERED_INSET : graphColumnWidth(geometry, layout.laneCount)
  const graphHeight = Math.max(1, layout.rows.length) * geometry.rowHeight
  const overlay = viewWidth > 0 && viewWidth < OVERLAY_BELOW_WIDTH
  const rowId = (hash: string) => `${idPrefix}-commit-${hash}`

  const select = useCallback(
    (hash: string) => {
      setSelectedHash(hash)
      requestAnimationFrame(() =>
        document.getElementById(`${idPrefix}-commit-${hash}`)?.scrollIntoView({ block: 'nearest' })
      )
    },
    [idPrefix]
  )

  const copyHash = useCallback(async (hash: string) => {
    const result = await clipboardApi.writeText(hash)
    if (result.success) {
      toast.success(`Copied ${hash.slice(0, 7)}`)
      return
    }
    void logFrontendError({
      level: 'warn',
      source: 'GitHistoryView.copyHash',
      message: `clipboard write failed: ${result.error}`
    })
    toast.error('Could not copy the hash')
  }, [])

  const openFile = useCallback(
    (hash: string, file: GitCommitFile) =>
      addCommitDiffTab(cwd, hash, file.path, file.oldPath ?? undefined),
    [addCommitDiffTab, cwd]
  )

  const openFirstFile = useCallback(() => {
    if (!selectedHash) return
    const entry = useGitHistoryStore.getState().details[commitDetailKey(cwd, selectedHash)]
    const first = entry?.detail?.files[0]
    if (first) openFile(selectedHash, first)
  }, [cwd, selectedHash, openFile])

  const moveSelection = (delta: number) => {
    if (visible.length === 0) return
    const index = visible.findIndex((c) => c.hash === selectedHash)
    const next = index < 0 ? 0 : Math.min(visible.length - 1, Math.max(0, index + delta))
    select(visible[next].hash)
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (isTypingTarget(event.target)) return
    const mod = event.metaKey || event.ctrlKey
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      moveSelection(event.key === 'ArrowDown' ? 1 : -1)
    } else if (event.key === 'Enter' && !mod) {
      openFirstFile()
    } else if (mod && event.key.toLowerCase() === 'c') {
      if (!selectedHash || (window.getSelection()?.toString() ?? '') !== '') return
      void copyHash(selectedHash)
    } else if (event.key === 'Escape' && detailOpen) {
      setDetailOpen(false)
    } else if (event.key === '/' && !mod) {
      filterRef.current?.focus()
    } else {
      return
    }
    event.preventDefault()
  }

  const handleFilterKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      if (query) setQuery('')
      else listRef.current?.focus()
    } else if (event.key === 'ArrowDown' || event.key === 'Enter') {
      event.preventDefault()
      if (visible.length > 0 && !visible.some((c) => c.hash === selectedHash))
        select(visible[0].hash)
      listRef.current?.focus()
    }
  }

  const countLabel = isFiltering
    ? `${visible.length} of ${all.length}`
    : `${all.length} ${all.length === 1 ? 'commit' : 'commits'}`

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: keyboard shortcuts scoped to the history tab; the list and controls inside are the focus targets.
    <div
      ref={rootRef}
      className="flex h-full w-full flex-col overflow-hidden bg-background"
      onKeyDown={handleKeyDown}
    >
      <HistoryToolbar
        scope={scope}
        onScopeChange={(next) => setScope(cwd, next)}
        query={query}
        onQueryChange={setQuery}
        filterRef={filterRef}
        onFilterKeyDown={handleFilterKeyDown}
        authors={authors}
        author={author}
        onAuthorChange={setAuthor}
        countLabel={countLabel}
        isLoading={isLoading}
        onRefresh={() => void refreshLog(cwd)}
        detailOpen={detailOpen}
        onToggleDetail={() => setDetailOpen((open) => !open)}
      />

      {commits === undefined && isLoading ? (
        <div className="flex flex-1 items-center justify-center text-xs text-muted-foreground">
          <Spinner size={16} decorative className="mr-2" />
          Loading history...
        </div>
      ) : all.length === 0 ? (
        <HistoryEmptyState error={error} />
      ) : (
        <div className="relative flex min-h-0 flex-1">
          <div
            ref={listRef}
            role="listbox"
            aria-label="Commits"
            aria-activedescendant={selectedHash ? rowId(selectedHash) : undefined}
            tabIndex={0}
            className="relative min-w-0 flex-1 overflow-auto focus-visible:outline-hidden"
          >
            <div
              className="sticky top-0 z-20 flex items-center border-b border-border bg-background pr-4 label-panel"
              style={{ height: HEADER_HEIGHT }}
            >
              <span className="shrink-0 truncate pl-3" style={{ width: graphWidth }}>
                {isFiltering ? '' : 'Graph'}
              </span>
              <span className="min-w-0 flex-1">Message</span>
              <span className="w-40 shrink-0 pl-3">Author</span>
              <span className="w-20 shrink-0 text-right">Date</span>
              <span className="w-20 shrink-0 pl-4">Hash</span>
            </div>
            <div className="relative" style={{ minHeight: isFiltering ? undefined : graphHeight }}>
              {isFiltering && visible.length === 0 ? (
                <p className="px-4 py-8 text-center text-xs text-muted-foreground">
                  No commits match this filter.
                </p>
              ) : (
                visible.map((commit) => (
                  <CommitTableRow
                    key={commit.hash}
                    id={rowId(commit.hash)}
                    commit={commit}
                    graphWidth={graphWidth}
                    rowHeight={geometry.rowHeight}
                    isSelected={commit.hash === selectedHash}
                    onSelect={() => {
                      setSelectedHash(commit.hash)
                      listRef.current?.focus()
                    }}
                    onOpen={openFirstFile}
                  />
                ))
              )}
              {!isFiltering && (
                <LaneGraph
                  layout={layout}
                  rowByHash={rowByHash}
                  geometry={geometry}
                  width={graphWidth}
                  height={graphHeight}
                />
              )}
            </div>
          </div>

          {detailOpen && selected && (
            <aside
              aria-label="Commit detail"
              className={cn(
                'flex shrink-0 flex-col border-l border-border bg-background',
                overlay ? 'absolute inset-y-0 right-0 z-30 shadow-xl' : 'relative'
              )}
              style={{
                width: overlay ? Math.min(pane.width, Math.max(0, viewWidth - 48)) : pane.width
              }}
            >
              {!overlay && (
                <button
                  type="button"
                  role="separator"
                  aria-orientation="vertical"
                  aria-label="Resize details"
                  aria-valuenow={pane.width}
                  aria-valuemin={DETAIL_PANE_MIN_WIDTH}
                  aria-valuemax={DETAIL_PANE_MAX_WIDTH}
                  onPointerDown={pane.onPointerDown}
                  onKeyDown={pane.onKeyDown}
                  title="Drag to resize details"
                  className={cn(
                    'absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize touch-none bg-transparent focus-visible:outline-none',
                    'after:absolute after:inset-y-0 after:left-1 after:w-px after:bg-transparent after:transition-colors after:duration-150 after:ease-out',
                    'hover:after:bg-muted-foreground/40 focus-visible:after:bg-muted-foreground/60 active:after:bg-muted-foreground/60'
                  )}
                />
              )}
              <CommitDetailPane
                cwd={cwd}
                commit={selected}
                onOpenFile={(file) => openFile(selected.hash, file)}
                onCopyHash={() => void copyHash(selected.hash)}
                onSelectCommit={(hash) => {
                  if (all.some((c) => c.hash === hash)) select(hash)
                  else toast('That commit is outside the loaded history.')
                }}
                onClose={overlay ? () => setDetailOpen(false) : undefined}
              />
            </aside>
          )}
        </div>
      )}
    </div>
  )
}

function CommitTableRow({
  id,
  commit,
  graphWidth,
  rowHeight,
  isSelected,
  onSelect,
  onOpen
}: {
  id: string
  commit: GitCommit
  graphWidth: number
  rowHeight: number
  isSelected: boolean
  onSelect: () => void
  onOpen: () => void
}): React.JSX.Element {
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the listbox owns the keyboard (aria-activedescendant).
    <div
      id={id}
      role="option"
      aria-selected={isSelected}
      tabIndex={-1}
      className={cn(
        'flex cursor-default items-center pr-4 text-xs transition-colors duration-150 ease-out',
        isSelected ? 'keycap rounded-md text-foreground' : 'hover:bg-foreground/[0.03]'
      )}
      style={{ height: rowHeight }}
      onClick={onSelect}
      onDoubleClick={onOpen}
    >
      <span className="shrink-0" style={{ width: graphWidth }} />
      <span className="flex min-w-0 flex-1 items-center gap-1.5">
        {commit.refs.map((ref) => (
          <RefChip key={ref} raw={ref} />
        ))}
        <span className="min-w-0 truncate text-foreground">{commit.subject}</span>
      </span>
      <span className="flex w-40 shrink-0 items-center gap-1.5 pl-3 text-muted-foreground">
        <AuthorAvatar name={commit.author} size={16} />
        <span className="truncate">{commit.author}</span>
      </span>
      <span
        className="w-20 shrink-0 text-right text-2xs tabular-nums text-muted-foreground"
        title={commit.date}
      >
        {formatRelativeTime(commit.date)}
      </span>
      <span className="w-20 shrink-0 pl-4 font-mono text-2xs text-muted-foreground">
        {commit.shortHash}
      </span>
    </div>
  )
}

function HistoryEmptyState({ error }: { error: string | null }): React.JSX.Element {
  return (
    <div className="flex flex-1 flex-col items-center justify-center p-8 text-center text-muted-foreground">
      <div className="mb-4 flex size-12 items-center justify-center rounded-full bg-secondary text-muted-foreground/50">
        <History size={24} />
      </div>
      <h3 className="mb-1 text-sm font-medium text-foreground">No commit history</h3>
      <p className="max-w-[260px] text-xs">
        {error
          ? 'This folder may not be a Git repository, or git is unavailable.'
          : 'There are no commits to show yet. Make your first commit to see it here.'}
      </p>
    </div>
  )
}
