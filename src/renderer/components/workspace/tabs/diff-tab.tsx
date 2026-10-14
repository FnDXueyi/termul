import { FileDiff } from '@/components/icons'
import { TabContextMenu } from '../tab-context-menu'
import { TabChrome } from './tab-chrome'
import type { TabInlineProps } from './types'

interface DiffTabInlineProps extends TabInlineProps {
  tab: { type: 'diff'; id: string; cwd: string; path: string; staged: boolean; commit?: string }
}

/** Repo-relative POSIX path -> its last segment (the tab label). */
function basename(path: string): string {
  const segments = path.split(/[/]/).filter(Boolean)
  return segments[segments.length - 1] ?? path
}

export function DiffTabInline({
  tab,
  isActive,
  isDragging,
  isDropTarget,
  dropPosition,
  onSelect,
  onClose,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
  bulkMenu
}: DiffTabInlineProps): React.JSX.Element {
  const shortHash = tab.commit?.slice(0, 7)
  const side = shortHash ? `at ${shortHash}` : tab.staged ? 'Staged' : 'Working tree'
  const name = basename(tab.path)
  const label = shortHash ? `${name} @ ${shortHash}` : tab.staged ? `${name} (staged)` : name
  return (
    <TabContextMenu kind="diff" onClose={onClose} {...bulkMenu}>
      <TabChrome
        isActive={isActive}
        isDragging={isDragging}
        isDropTarget={isDropTarget}
        dropPosition={dropPosition}
        onSelect={onSelect}
        onClose={onClose}
        onDragStart={onDragStart}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
        icon={<FileDiff size={12} />}
        label={label}
        title={`${tab.path} (${side})`}
        pinClose={isActive}
      />
    </TabContextMenu>
  )
}
