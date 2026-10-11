import { FileDiff } from '@/components/icons'
import { TabContextMenu } from '../tab-context-menu'
import { TabChrome } from './tab-chrome'
import type { TabInlineProps } from './types'

interface DiffTabInlineProps extends TabInlineProps {
  tab: { type: 'diff'; id: string; cwd: string; path: string; staged: boolean }
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
  const side = tab.staged ? 'Staged' : 'Working tree'
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
        label={tab.staged ? `${basename(tab.path)} (staged)` : basename(tab.path)}
        title={`${tab.path} (${side})`}
        pinClose={isActive}
      />
    </TabContextMenu>
  )
}
