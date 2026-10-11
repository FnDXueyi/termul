import { GitDiffView } from '@/components/git/GitDiffView'
import type { GitActions } from '@/components/git/use-git-actions'
import {
  AlertTriangle,
  AlignLeft,
  ChevronLeft,
  Columns2,
  FileCode,
  FileText
} from '@/components/icons'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Spinner } from '@/components/ui/spinner'
import { type GitDiffViewMode, saveGitDiffViewMode } from '@/lib/parse-unified-diff'

interface GitDiffPaneProps {
  variant: 'desktop' | 'mobile'
  selectedFile: string
  selectedStaged: boolean
  /** `undefined`/`null` while the diff is loading. */
  currentDiff: string | null | undefined
  diffViewMode: GitDiffViewMode
  onDiffViewModeChange: (mode: GitDiffViewMode) => void
  onStageHunk: GitActions['runStageHunk']
  onUnstageHunk: GitActions['runUnstageHunk']
  /** Mobile only: returns to the file list. */
  onBack?: () => void
  /** A failed load: shown instead of the loading / empty states, with a Retry. */
  failure?: { detail: string; onRetry: () => void }
  /** Disables the hunk and selected-line controls (read-only server, write in flight). */
  controlsDisabled?: boolean
  /** Why the controls are disabled; shown under the header when set. */
  disabledReason?: string | null
}

/** Diff header (file name, view-mode toggle, side label) and diff body. */
export function GitDiffPane({
  variant,
  selectedFile,
  selectedStaged,
  currentDiff,
  diffViewMode,
  onDiffViewModeChange,
  onStageHunk,
  onUnstageHunk,
  onBack,
  failure,
  controlsDisabled = false,
  disabledReason = null
}: GitDiffPaneProps) {
  const isMobile = variant === 'mobile'
  const iconSize = isMobile ? 16 : 14
  const toggleSize = isMobile ? 'touch' : 'icon'
  const toggleClassName = isMobile ? 'min-h-9 min-w-9' : 'h-7 w-7'

  return (
    <>
      {/* Mobile: pr-14 reserves the sheet close target (absolute, top-right, 44px on
          coarse pointers) so it never covers the header's right edge. */}
      <div
        className={
          isMobile
            ? 'border-b border-border bg-background p-2 pr-14 flex items-center justify-between gap-2'
            : 'p-3 border-b border-border flex items-center justify-between gap-2 bg-background'
        }
      >
        {isMobile && (
          <Button
            type="button"
            variant="ghost"
            size="touch"
            className="shrink-0"
            aria-label="Back to file list"
            onClick={onBack}
          >
            <ChevronLeft size={18} />
          </Button>
        )}
        <div className="flex items-center gap-3 overflow-hidden min-w-0">
          <FileCode size={16} className="text-primary shrink-0" />
          <span className="text-sm font-medium truncate">{selectedFile}</span>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <div
            className="flex items-center rounded-md border border-border p-0.5"
            role="group"
            aria-label="Diff view mode"
          >
            <Button
              type="button"
              variant={diffViewMode === 'inline' ? 'secondary' : 'ghost'}
              size={toggleSize}
              className={toggleClassName}
              title="Inline diff"
              aria-pressed={diffViewMode === 'inline'}
              onClick={() => {
                onDiffViewModeChange('inline')
                saveGitDiffViewMode('inline')
              }}
            >
              <AlignLeft size={iconSize} />
            </Button>
            <Button
              type="button"
              variant={diffViewMode === 'split' ? 'secondary' : 'ghost'}
              size={toggleSize}
              className={toggleClassName}
              title="Side-by-side diff"
              aria-pressed={diffViewMode === 'split'}
              onClick={() => {
                onDiffViewModeChange('split')
                saveGitDiffViewMode('split')
              }}
            >
              <Columns2 size={iconSize} />
            </Button>
          </div>
          <span className="label-group text-muted-foreground">
            {selectedStaged ? 'Staged' : 'Working tree'}
          </span>
        </div>
      </div>
      {disabledReason && (
        <p className="border-b border-border bg-background px-3 py-1.5 text-xs text-muted-foreground">
          {disabledReason}
        </p>
      )}
      <ScrollArea className="flex-1 font-mono text-xs">
        {failure ? (
          <div className="h-full flex flex-col items-center justify-center text-muted-foreground p-8 text-center">
            <div className="w-10 h-10 rounded-full bg-secondary flex items-center justify-center mb-3 text-muted-foreground/60">
              <AlertTriangle size={18} />
            </div>
            <h3 className="text-sm font-medium text-foreground mb-1">Couldn't load this diff.</h3>
            <p className="text-xs max-w-[320px] break-words mb-3">{failure.detail}</p>
            <Button type="button" variant="secondary" size="sm" onClick={failure.onRetry}>
              Retry
            </Button>
          </div>
        ) : currentDiff === undefined || currentDiff === null ? (
          <div className="h-full flex items-center justify-center text-muted-foreground">
            <Spinner size={16} decorative className="mr-2" />
            Loading diff...
          </div>
        ) : currentDiff.trim().length > 0 ? (
          <fieldset
            disabled={controlsDisabled}
            aria-disabled={controlsDisabled || undefined}
            className="contents"
          >
            <GitDiffView
              diff={currentDiff}
              mode={diffViewMode}
              filePath={selectedFile}
              diffSide={selectedStaged ? 'staged' : 'unstaged'}
              onStageHunk={onStageHunk}
              onUnstageHunk={onUnstageHunk}
            />
          </fieldset>
        ) : (
          <div className="h-full flex flex-col items-center justify-center text-muted-foreground p-8 text-center">
            <div className="w-10 h-10 rounded-full bg-secondary flex items-center justify-center mb-3 text-muted-foreground/60">
              <FileText size={18} />
            </div>
            <h3 className="text-sm font-medium text-foreground mb-1">No diff available</h3>
            <p className="text-xs max-w-[260px]">
              This file may be ignored by Git, unchanged relative to the selected base, or
              unavailable for diff preview.
            </p>
          </div>
        )}
      </ScrollArea>
    </>
  )
}
