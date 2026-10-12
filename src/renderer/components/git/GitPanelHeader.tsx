import type React from 'react'
import { BranchSection } from '@/components/git/BranchSection'
import { Archive, Search } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { WriteGate } from '@/components/ui/write-gate'
import { useServerAdmitsRemoteWrites } from '@/hooks/use-server-admits-remote-writes'
import { cn } from '@/lib/utils'

interface GitPanelHeaderProps {
  /** `column` is the desktop look inside the shell Changes panel: the branch
      row, then the `belowBranch` slot (the commit box), then the filter field. */
  variant: 'desktop' | 'mobile' | 'column'
  branches: string[]
  currentBranch: string | null | undefined
  hasUncommittedChanges: boolean
  isGenerating: boolean
  searchQuery: string
  onSearchQueryChange: (value: string) => void
  onCreateBranch: () => void
  onSwitchBranch: (name: string) => void
  onOpenStash: () => void
  /** `column` only: rendered between the branch row and the filter field. */
  belowBranch?: React.ReactNode
}

/** Branch picker + Stash button + "Filter changes..." input. */
export function GitPanelHeader({
  variant,
  branches,
  currentBranch,
  hasUncommittedChanges,
  isGenerating,
  searchQuery,
  onSearchQueryChange,
  onCreateBranch,
  onSwitchBranch,
  onOpenStash,
  belowBranch
}: GitPanelHeaderProps) {
  const admitted = useServerAdmitsRemoteWrites()
  if (variant === 'column') {
    return (
      <div className="flex flex-col bg-muted/20">
        <div
          className={cn(
            'p-3 flex items-center justify-between',
            !belowBranch && 'border-b border-border'
          )}
        >
          <BranchSection
            branches={branches}
            currentBranch={currentBranch}
            onCreateBranch={onCreateBranch}
            onSwitchBranch={onSwitchBranch}
          />

          <WriteGate gated={!admitted}>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-muted-foreground hover:text-foreground hover:bg-secondary"
              title="Stash changes"
              onClick={onOpenStash}
              disabled={!hasUncommittedChanges || isGenerating || !admitted}
            >
              <Archive size={14} />
            </Button>
          </WriteGate>
        </div>

        {belowBranch}

        <div className={cn('p-3 border-b border-border', belowBranch && 'border-t')}>
          <div className="relative">
            <Search
              className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
              size={14}
            />
            <input
              type="text"
              placeholder="Filter changes..."
              className={`w-full bg-secondary/50 border-none rounded-md py-1.5 pl-8 pr-3 text-xs pointer-coarse:text-base ${FOCUS_RING_CLASS}`}
              value={searchQuery}
              onChange={(e) => onSearchQueryChange(e.target.value)}
            />
          </div>
        </div>
      </div>
    )
  }

  if (variant === 'mobile') {
    return (
      // pr-14 reserves the sheet close target so it does not overlap the
      // Stash changes button at the right edge of the first row.
      <div className="p-2 pr-14 border-b border-border flex flex-col gap-2 bg-muted/20">
        <div className="flex items-center justify-between">
          <BranchSection
            variant="mobile"
            branches={branches}
            currentBranch={currentBranch}
            onCreateBranch={onCreateBranch}
            onSwitchBranch={onSwitchBranch}
          />

          <WriteGate gated={!admitted}>
            <Button
              variant="ghost"
              size="touch"
              className="w-11 text-muted-foreground hover:text-foreground hover:bg-secondary"
              title="Stash changes"
              onClick={onOpenStash}
              disabled={!hasUncommittedChanges || isGenerating || !admitted}
            >
              <Archive size={16} />
            </Button>
          </WriteGate>
        </div>

        <div className="relative">
          <Search
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
            size={14}
          />
          <input
            type="text"
            placeholder="Filter changes..."
            className={`w-full bg-secondary/50 border-none rounded-md py-2.5 pl-8 pr-3 text-xs pointer-coarse:text-base ${FOCUS_RING_CLASS}`}
            value={searchQuery}
            onChange={(e) => onSearchQueryChange(e.target.value)}
          />
        </div>
      </div>
    )
  }

  return (
    <div className="p-3 border-b border-border flex flex-col gap-2 bg-muted/20">
      <div className="flex items-center justify-between">
        <BranchSection
          branches={branches}
          currentBranch={currentBranch}
          onCreateBranch={onCreateBranch}
          onSwitchBranch={onSwitchBranch}
        />

        <WriteGate gated={!admitted}>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-muted-foreground hover:text-foreground hover:bg-secondary"
            title="Stash changes"
            onClick={onOpenStash}
            disabled={!hasUncommittedChanges || isGenerating || !admitted}
          >
            <Archive size={14} />
          </Button>
        </WriteGate>
      </div>

      <div className="relative">
        <Search
          className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
          size={14}
        />
        <input
          type="text"
          placeholder="Filter changes..."
          className={`w-full bg-secondary/50 border-none rounded-md py-1.5 pl-8 pr-3 text-xs pointer-coarse:text-base ${FOCUS_RING_CLASS}`}
          value={searchQuery}
          onChange={(e) => onSearchQueryChange(e.target.value)}
        />
      </div>
    </div>
  )
}
