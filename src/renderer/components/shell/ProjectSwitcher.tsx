import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { railButtonClass } from '@/components/ActivityRail'
import { Check, Plus } from '@/components/icons'
import { ProjectIcon } from '@/components/ProjectIcon'
import {
  Command,
  CommandGroup,
  CommandItem,
  CommandList,
  CommandSeparator
} from '@/components/ui/command'
import { MENU_CONTENT_CLASS, MENU_ITEM_CLASS } from '@/components/ui/menu-styles'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { PopoverSearchBand } from '@/components/ui/popover-search-band'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useNativeWebviewOcclusion } from '@/hooks/use-native-webview-occlusion'
import { cn } from '@/lib/utils'
import type { Project } from '@/types/project'

/**
 * cmdk marks every item `data-disabled="false"`, and `MENU_ITEM_CLASS` matches
 * the bare `data-disabled` attribute (Radix sets it only when disabled), which
 * would make every row ignore the pointer. Re-enable the pointer on the
 * not-disabled state.
 */
const SWITCHER_ITEM_CLASS = cn(
  MENU_ITEM_CLASS,
  'gap-2 data-[disabled=false]:pointer-events-auto! data-[disabled=false]:opacity-100!'
)

interface ProjectSwitcherProps {
  projects: Project[]
  activeProject: Project | undefined
  onSelectProject: (id: string) => void
  onNewProject: () => void
}

/**
 * Collapsed Rail's Project switcher: the active Project's icon opens a
 * popover with a search field, the open Projects and `New project`. Enter
 * switches (as the sidebar's `openProject`) and closes; Esc closes with focus
 * back on the icon (Radix restores it to the trigger).
 */
export function ProjectSwitcher({
  projects,
  activeProject,
  onSelectProject,
  onNewProject
}: ProjectSwitcherProps): React.JSX.Element {
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  useNativeWebviewOcclusion(open)

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase()
    const live = projects.filter((project) => !project.isArchived)
    if (!needle) return live
    return live.filter((project) => project.name.toLowerCase().includes(needle))
  }, [projects, query])

  const handleOpenChange = (next: boolean): void => {
    setOpen(next)
    if (!next) setQuery('')
  }

  const selectProject = (projectId: string): void => {
    handleOpenChange(false)
    onSelectProject(projectId)
    navigate('/')
  }

  const label = activeProject ? `Switch project (${activeProject.name})` : 'Switch project'

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              type="button"
              className={railButtonClass(open)}
              aria-label={label}
              aria-haspopup="dialog"
              aria-expanded={open}
            >
              {activeProject ? (
                <ProjectIcon project={activeProject} size={20} />
              ) : (
                <span
                  aria-hidden="true"
                  className="size-5 rounded border border-dashed border-muted-foreground/60"
                />
              )}
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="right" className="px-2.5 py-1.5 text-xs">
          <span className="text-popover-foreground">{label}</span>
        </TooltipContent>
      </Tooltip>

      <PopoverContent
        side="right"
        align="start"
        sideOffset={8}
        className={cn(MENU_CONTENT_CLASS, 'w-72 p-0')}
        aria-label="Switch project"
      >
        {/* Filtering is by name here, so cmdk's own filter is off. */}
        <Command shouldFilter={false} className="rounded-xl">
          <PopoverSearchBand
            value={query}
            onChange={setQuery}
            placeholder="Search projects…"
            ariaLabel="Search projects"
            autoFocus
            inputClassName="text-xs"
          />
          <CommandList className="max-h-72 p-1">
            {matches.length === 0 ? (
              <p role="status" className="py-6 text-center text-xs text-muted-foreground">
                No projects found
              </p>
            ) : (
              <CommandGroup className="p-0">
                {matches.map((project) => {
                  const isActive = project.id === activeProject?.id
                  return (
                    <CommandItem
                      key={project.id}
                      value={project.id}
                      onSelect={() => selectProject(project.id)}
                      className={SWITCHER_ITEM_CLASS}
                      aria-current={isActive ? 'true' : undefined}
                    >
                      <ProjectIcon project={project} size={16} />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-medium">{project.name}</span>
                        {project.path && (
                          <span className="block truncate text-2xs text-muted-foreground">
                            {project.path}
                          </span>
                        )}
                      </span>
                      {isActive && (
                        <Check
                          size={14}
                          aria-label="Active project"
                          className="shrink-0 text-foreground"
                        />
                      )}
                    </CommandItem>
                  )
                })}
              </CommandGroup>
            )}
            <CommandSeparator className="my-1" />
            <CommandItem
              value="__new-project__"
              onSelect={() => {
                handleOpenChange(false)
                onNewProject()
              }}
              className={SWITCHER_ITEM_CLASS}
            >
              <Plus size={14} aria-hidden="true" className="shrink-0 text-muted-foreground" />
              New project
            </CommandItem>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
