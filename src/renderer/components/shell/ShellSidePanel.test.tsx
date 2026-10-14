import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TopRowProvider } from '@/components/shell/top-row-context'
import type { SSHWorkspaceState } from '@/layouts/workspace-layout/use-ssh-workspace'
import type { TopRowPlatform, TopRowRoles } from '@/lib/top-row'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import type { Project } from '@/types/project'
import { ShellSidePanel } from './ShellSidePanel'

vi.mock('@/components/shell/SidePanelColumn', () => ({
  SidePanelColumn: ({
    activeProject,
    dragRegion,
    trafficLightInset
  }: {
    activeProject: Project
    dragRegion?: boolean
    trafficLightInset?: boolean
  }) => (
    <div
      data-testid="side-panel-column"
      data-drag={String(Boolean(dragRegion))}
      data-inset={String(Boolean(trafficLightInset))}
    >
      {activeProject.id}
    </div>
  )
}))
vi.mock('@/components/shell/ShellRail', () => ({ COLLAPSED_WIDTH: 48 }))
vi.mock('@/layouts/workspace-layout/ExplorerColumn', () => ({
  ExplorerColumn: ({ sidePanel }: { sidePanel?: React.ReactNode }) => (
    <div data-testid="explorer-column">{sidePanel}</div>
  )
}))

const ssh = {} as SSHWorkspaceState
const roles: TopRowRoles = {
  topEdgeLeafIds: new Set(),
  rightmostLeafId: null,
  leftmostLeafId: null,
  controlsOwner: 'pane'
}
const desktop: TopRowPlatform = { dragRegions: true, windowControls: true, trafficLights: false }
const mac: TopRowPlatform = { dragRegions: true, windowControls: false, trafficLights: true }

function renderPanel(
  platform: TopRowPlatform | null,
  project: Project | null = { id: 'p1' } as Project
) {
  const panel = <ShellSidePanel activeProject={project ?? undefined} ssh={ssh} />
  return render(
    platform ? (
      <TopRowProvider roles={roles} platform={platform}>
        {panel}
      </TopRowProvider>
    ) : (
      panel
    )
  )
}

beforeEach(() => {
  resetShellLayoutStoreForTests()
  useShellLayoutStore.setState({ status: 'unavailable' })
})

describe('ShellSidePanel', () => {
  it('injects the Column tabs panel into the explorer column', () => {
    renderPanel(null)
    expect(screen.getByTestId('side-panel-column')).toHaveTextContent('p1')
  })

  it('injects nothing without an active project', () => {
    renderPanel(null, null)
    expect(screen.queryByTestId('side-panel-column')).toBeNull()
  })

  describe('Rail expanded', () => {
    it('returns the explorer column as is: no stack, no strip', () => {
      const { container } = renderPanel(desktop)
      expect(container.firstElementChild).toBe(screen.getByTestId('explorer-column'))
      expect(screen.queryByTestId('side-panel-stack')).toBeNull()
      expect(screen.queryByTestId('top-row-toggle-strip')).toBeNull()
    })

    it('makes the column header a drag region on desktop only', () => {
      const { unmount } = renderPanel(desktop)
      expect(screen.getByTestId('side-panel-column')).toHaveAttribute('data-drag', 'true')
      unmount()

      renderPanel({ dragRegions: false, windowControls: false, trafficLights: false })
      expect(screen.getByTestId('side-panel-column')).toHaveAttribute('data-drag', 'false')
    })

    it('does not make the header a drag region without a provider', () => {
      renderPanel(null)
      expect(screen.getByTestId('side-panel-column')).toHaveAttribute('data-drag', 'false')
    })
  })

  describe('Rail collapsed', () => {
    beforeEach(() => {
      useShellLayoutStore.setState({ railExpanded: false })
    })

    it('returns the explorer column as is: nothing above it, so a hidden panel leaves no gap', () => {
      const { container } = renderPanel(desktop)
      expect(container.firstElementChild).toBe(screen.getByTestId('explorer-column'))
      expect(screen.queryByTestId('top-row-toggle-strip')).toBeNull()
    })

    it('keeps the header in the top row (drag region on desktop)', () => {
      renderPanel(desktop)
      expect(screen.getByTestId('side-panel-column')).toHaveAttribute('data-drag', 'true')
    })

    it('adds the traffic-light inset on macOS only', () => {
      const { unmount } = renderPanel(mac)
      expect(screen.getByTestId('side-panel-column')).toHaveAttribute('data-inset', 'true')
      unmount()

      renderPanel(desktop)
      expect(screen.getByTestId('side-panel-column')).toHaveAttribute('data-inset', 'false')
    })
  })

  it('adds no inset with the Rail expanded, even on macOS', () => {
    renderPanel(mac)
    expect(screen.getByTestId('side-panel-column')).toHaveAttribute('data-inset', 'false')
  })
})
