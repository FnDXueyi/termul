import { Button } from '@/components/ui/button'

interface ChangesTabBodyProps {
  /** Working copy whose Git changes the button opens. */
  cwd: string
  onOpenGitChanges: (cwd: string) => void
}

/**
 * Changes tab body (bridge until the Changes panel lands): one muted line and a
 * button that opens today's Git tab. `GitPanel` is not mounted here because
 * its two-pane layout does not fit a column.
 */
export function ChangesTabBody({ cwd, onOpenGitChanges }: ChangesTabBodyProps): React.JSX.Element {
  return (
    <div className="flex flex-col items-start gap-2 px-4 py-2">
      <p className="text-xs text-muted-foreground">Review and commit changes in the Git tab.</p>
      <Button type="button" variant="outline" size="xs" onClick={() => onOpenGitChanges(cwd)}>
        Open Git changes
      </Button>
    </div>
  )
}
