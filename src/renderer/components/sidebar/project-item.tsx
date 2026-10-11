import type { GitDiffStat } from '@shared/types/ipc.types'
import { type KeyboardEvent, memo, useEffect, useRef } from 'react'
import { Settings } from '@/components/icons'
import { ProjectIcon } from '@/components/ProjectIcon'
import { RAIL_ICON_BOX_CLASS, RAIL_ROW_INSET_CLASS } from '@/components/shell/rail-layout'
import { ContextMenu, ContextMenuTrigger } from '@/components/ui/context-menu'
import { Kbd } from '@/components/ui/kbd'
import { cn } from '@/lib/utils'
import type { Project } from '@/types/project'
import { ProjectDiffStat } from '../shell/ProjectDiffStat'
import { type ProjectRowStatus, ProjectStatusMarks } from './indicators'

export interface ProjectItemProps {
  project: Project
  isActive: boolean
  isEditing: boolean
  editName: string
  shortcut?: string
  status: ProjectRowStatus
  onOpenNeedsYou: () => void
  onClick: () => void
  onContextMenu: (e: React.MouseEvent) => void
  onEditNameChange: (name: string) => void
  onSaveRename: () => void
  onCancelRename: () => void
  onSettingsClick: () => void
  renderContextMenu?: (project: Project) => React.ReactNode
  /** Uncommitted line stat for the row (see `RailProjectItem`); nothing when null. */
  diffStat?: GitDiffStat | null
}

export const ProjectItem = memo(function ProjectItem({
  project,
  isActive,
  isEditing,
  editName,
  shortcut,
  status,
  onOpenNeedsYou,
  onClick,
  onContextMenu,
  onEditNameChange,
  onSaveRename,
  onCancelRename,
  onSettingsClick,
  renderContextMenu,
  diffStat: watchedDiffStat = null
}: ProjectItemProps): React.JSX.Element {
  const inputRef = useRef<HTMLInputElement>(null)
  // At rest the stat holds the right slot; hover or focus swaps it for the
  // settings gear and shortcut chip. It yields to the rename input.
  const diffStat = isEditing ? null : watchedDiffStat

  // Focus input when editing starts
  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus()
      inputRef.current.select()
    }
  }, [isEditing])

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === 'Enter') {
      e.preventDefault()
      onSaveRename()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      onCancelRename()
    }
  }

  return (
    <div data-testid={`project-item-${project.id}`}>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            onClick={isEditing ? undefined : onClick}
            onContextMenu={onContextMenu}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault()
                if (!isEditing) onClick()
              }
            }}
            className={cn(
              'group flex h-8 w-full cursor-pointer select-none items-center pr-1.5 text-left text-xs',
              'rounded-lg',
              RAIL_ROW_INSET_CLASS,
              'transition-colors duration-150 ease-out focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
              isActive ? 'keycap text-foreground' : 'hover:bg-foreground/[0.03]'
            )}
            aria-current={isActive ? 'page' : undefined}
            aria-label={`Project: ${project.name}${isActive ? ' (active)' : ''}`}
          >
            <span data-rail-icon-column className={RAIL_ICON_BOX_CLASS}>
              <ProjectIcon project={project} size={16} />
            </span>

            {isEditing ? (
              <input
                ref={inputRef}
                type="text"
                value={editName}
                onChange={(e) => onEditNameChange(e.target.value)}
                onKeyDown={handleKeyDown}
                onBlur={onSaveRename}
                className="h-6 min-w-0 flex-1 rounded border border-ring bg-card px-1.5 text-xs text-foreground outline-none"
                onClick={(e) => e.stopPropagation()}
              />
            ) : (
              <span
                className={cn(
                  // flex-1 min-w-0 is required for truncate to clip inside a flex row
                  'min-w-0 flex-1 truncate font-medium transition-colors duration-150 ease-out',
                  isActive
                    ? 'text-foreground'
                    : 'text-secondary-foreground group-hover:text-foreground'
                )}
                title={project.name}
              >
                {project.name}
              </span>
            )}
            {/* Right slot: status marks, then (hover/active) settings + shortcut.
                The activity spinner yields to the rename input. */}
            <ProjectStatusMarks
              status={isEditing && status.live === 'activity' ? { ...status, live: null } : status}
              onOpenNeedsYou={onOpenNeedsYou}
            />
            {diffStat && (
              <span className="flex shrink-0 group-hover:hidden group-focus-within:hidden">
                <ProjectDiffStat stat={diffStat} />
              </span>
            )}
            {!isEditing && (
              <span
                className={cn(
                  'shrink-0 items-center gap-1 transition-opacity duration-150 ease-out',
                  diffStat ? 'hidden group-hover:flex group-focus-within:flex' : 'flex',
                  isActive && !diffStat
                    ? 'opacity-100'
                    : 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'
                )}
              >
                <button
                  onClick={(e) => {
                    e.stopPropagation()
                    onSettingsClick()
                  }}
                  className="inline-flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors duration-150 ease-out hover:bg-foreground/[0.03] hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  title="Project settings"
                  aria-label={`Settings for ${project.name}`}
                >
                  <Settings size={12} />
                </button>
                {shortcut && (
                  <Kbd className="inline-flex h-4.5 shrink-0 items-center px-1">{shortcut}</Kbd>
                )}
              </span>
            )}
          </div>
        </ContextMenuTrigger>
        {renderContextMenu?.(project)}
      </ContextMenu>
    </div>
  )
})
