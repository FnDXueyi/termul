import { memo } from 'react'
import { useProjectDiffStat } from '../shell/ProjectDiffStat'
import { ProjectItem, type ProjectItemProps } from './project-item'

/**
 * Project row of the shell Rail: today's row in its `rail` look, plus its
 * uncommitted diff stat. The row watches its working copy in the git-status
 * store while mounted and shows nothing when there is no stat to show.
 */
export const RailProjectItem = memo(function RailProjectItem(
  props: Omit<ProjectItemProps, 'variant' | 'diffStat'>
): React.JSX.Element {
  const diffStat = useProjectDiffStat(props.project, true)
  return <ProjectItem {...props} variant="rail" diffStat={diffStat} />
})
