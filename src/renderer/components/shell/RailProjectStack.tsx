import { useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { AlertTriangle } from '@/components/icons'
import { ProjectIcon } from '@/components/ProjectIcon'
import { ProjectSwitcher } from '@/components/shell/ProjectSwitcher'
import { RailButton } from '@/components/shell/rail-button'
import type { ProjectRowStatus } from '@/components/sidebar/indicators'
import { useProjectRowStatus } from '@/components/sidebar/use-project-row-status'
import { needsYouLabel } from '@/lib/agent-chat-attention'
import type { Project } from '@/types/project'

/** Other projects shown as icons under the active one; the rest go behind "+N". */
export const RAIL_STACK_OTHERS = 4

interface RailProjectStackProps {
  projects: Project[]
  activeProject: Project | undefined
  onSelectProject: (id: string) => void
  onNewProject: () => void
}

/**
 * The other projects to show, in list order so an icon never moves under the
 * pointer when a status changes. When a project behind "+N" needs you and no
 * shown one does, it takes the last visible slot.
 */
export function pickShownProjects<T extends { status: ProjectRowStatus }>(
  others: T[],
  limit: number
): T[] {
  const shown = others.slice(0, limit)
  if (others.length <= limit || shown.some((o) => o.status.attentionCount > 0)) return shown
  const urgent = others.slice(limit).find((o) => o.status.attentionCount > 0)
  return urgent ? [...shown.slice(0, limit - 1), urgent] : shown
}

/** Tooltip and accessible-name suffix for the one badge a project icon shows. */
function statusHint(status: ProjectRowStatus): string | undefined {
  if (status.attentionCount > 0) return needsYouLabel(status.attentionCount)
  if (status.crashed) return 'Terminal crashed'
  if (status.live === 'activity') return 'Running'
  return undefined
}

/**
 * One badge on the icon's top-right corner, in the same order as the
 * expanded row: needs you (warning count), crash (warning glyph), live work
 * (primary dot, blue is for live work only).
 */
function StatusBadge({ status }: { status: ProjectRowStatus }): React.JSX.Element | null {
  if (status.attentionCount > 0) {
    return (
      <span
        aria-hidden="true"
        className="pointer-events-none absolute -top-1 -right-1.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-warning px-1 text-3xs font-semibold tabular-nums text-warning-foreground ring-2 ring-background"
      >
        {status.attentionCount > 9 ? '9+' : status.attentionCount}
      </span>
    )
  }
  if (status.crashed) {
    return (
      <span
        aria-hidden="true"
        className="pointer-events-none absolute -top-0.5 -right-0.5 flex size-3.5 items-center justify-center rounded-full bg-background text-warning"
      >
        <AlertTriangle size={10} />
      </span>
    )
  }
  if (status.live === 'activity') {
    return (
      <span
        aria-hidden="true"
        className="pointer-events-none absolute top-1.5 right-1.5 size-1.5 rounded-full bg-primary ring-2 ring-background"
      />
    )
  }
  return null
}

/**
 * Collapsed Rail projects: the active project (the Project switcher trigger,
 * on the keycap), then up to {@link RAIL_STACK_OTHERS} other open projects
 * with their status in list order, then "+N" for the rest (the same
 * switcher). See {@link pickShownProjects} for the one exception.
 */
export function RailProjectStack({
  projects,
  activeProject,
  onSelectProject,
  onNewProject
}: RailProjectStackProps): React.JSX.Element {
  const navigate = useNavigate()
  const projectStatus = useProjectRowStatus()

  const { shown, hiddenCount } = useMemo(() => {
    const others = projects
      .filter((project) => !project.isArchived && project.id !== activeProject?.id)
      .map((project) => ({ project, status: projectStatus(project.id) }))
    return {
      shown: pickShownProjects(others, RAIL_STACK_OTHERS),
      hiddenCount: Math.max(0, others.length - RAIL_STACK_OTHERS)
    }
  }, [projects, activeProject?.id, projectStatus])

  const switcherProps = { projects, activeProject, onSelectProject, onNewProject }

  return (
    <div className="flex flex-col items-center gap-0.5" data-testid="rail-project-stack">
      <ProjectSwitcher {...switcherProps} pressed={Boolean(activeProject)} />

      {shown.map(({ project, status }) => {
        const hint = statusHint(status)
        return (
          <RailButton
            key={project.id}
            label={project.name}
            hint={hint}
            aria-label={hint ? `Open ${project.name}, ${hint}` : `Open ${project.name}`}
            badge={<StatusBadge status={status} />}
            onClick={() => {
              onSelectProject(project.id)
              navigate('/')
            }}
          >
            <ProjectIcon project={project} size={20} />
          </RailButton>
        )
      })}

      {hiddenCount > 0 && (
        <ProjectSwitcher {...switcherProps} label={`${hiddenCount} more projects`}>
          <span className="text-2xs font-semibold tabular-nums text-muted-foreground">
            +{hiddenCount}
          </span>
        </ProjectSwitcher>
      )}
    </div>
  )
}
