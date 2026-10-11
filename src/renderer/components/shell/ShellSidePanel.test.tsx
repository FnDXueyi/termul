import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { SSHWorkspaceState } from '@/layouts/workspace-layout/use-ssh-workspace'
import type { Project } from '@/types/project'
import { ShellSidePanel } from './ShellSidePanel'

vi.mock('@/components/shell/SidePanelColumn', () => ({
  SidePanelColumn: ({ activeProject }: { activeProject: Project }) => (
    <div data-testid="side-panel-column">{activeProject.id}</div>
  )
}))
vi.mock('@/layouts/workspace-layout/ExplorerColumn', () => ({
  ExplorerColumn: ({ side, sidePanel }: { side?: string; sidePanel?: React.ReactNode }) => (
    <div data-testid="explorer-column" data-side={side}>
      {sidePanel}
    </div>
  )
}))

const ssh = {} as SSHWorkspaceState

describe('ShellSidePanel', () => {
  it('injects the Column tabs panel into the explorer column on the left', () => {
    render(<ShellSidePanel activeProject={{ id: 'p1' } as Project} ssh={ssh} />)
    expect(screen.getByTestId('explorer-column')).toHaveAttribute('data-side', 'left')
    expect(screen.getByTestId('side-panel-column')).toHaveTextContent('p1')
  })

  it('injects nothing without an active project', () => {
    render(<ShellSidePanel activeProject={undefined} ssh={ssh} />)
    expect(screen.queryByTestId('side-panel-column')).toBeNull()
  })
})
