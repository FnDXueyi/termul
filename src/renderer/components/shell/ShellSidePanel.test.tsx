import { render, screen, within } from '@testing-library/react'
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
    dragRegion
  }: {
    activeProject: Project
    dragRegion?: boolean
  }) => (
    <div data-testid="side-panel-column" data-drag={String(Boolean(dragRegion))}>
      {activeProject.id}
    </div>
  )
}))
vi.mock('@/components/shell/TopRowToggles', () => ({
  RailToggleButton: () => <button type="button">rail-toggle</button>,
  SidePanelToggleButton: () => <button type="button">side-toggle</button>
}))
vi.mock('@/components/shell/ShellRail', () => ({ COLLAPSED_WIDTH: 48 }))
vi.mock('@/layouts/workspace-layout/ExplorerColumn', () => ({
  ExplorerColumn: ({ side, sidePanel }: { side?: string; sidePanel?: React.ReactNode }) => (
    <div data-testid="explorer-column" data-side={side}>
      {sidePanel}
    </div>
  )
}))

const ssh = {} as SSHWorkspaceState
const roles: TopRowRoles = {
  topEdgeLeafIds: new Set(),
  rightmostLeafId: null,
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
  it('injects the Column tabs panel into the explorer column on the left', () => {
    renderPanel(null)
    expect(screen.getByTestId('explorer-column')).toHaveAttribute('data-side', 'left')
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

    it('stacks the toggle strip above the column; the header is not a drag region', () => {
      renderPanel(desktop)
      const stack = screen.getByTestId('side-panel-stack')
      const strip = within(stack).getByTestId('top-row-toggle-strip')
      expect(within(strip).getByText('rail-toggle')).toBeInTheDocument()
      expect(within(strip).getByText('side-toggle')).toBeInTheDocument()
      expect(
        strip.compareDocumentPosition(within(stack).getByTestId('explorer-column')) &
          Node.DOCUMENT_POSITION_FOLLOWING
      ).toBeTruthy()
      expect(screen.getByTestId('side-panel-column')).toHaveAttribute('data-drag', 'false')
    })

    it('keeps the strip when there is no column to show', () => {
      renderPanel(desktop, null)
      expect(screen.getByTestId('top-row-toggle-strip')).toBeInTheDocument()
      expect(screen.queryByTestId('side-panel-column')).toBeNull()
    })

    it('marks the strip root and its spacer as drag regions, never the toggles', () => {
      renderPanel(desktop)
      const strip = screen.getByTestId('top-row-toggle-strip')
      expect(strip).toHaveAttribute('data-tauri-drag-region')
      expect(strip.querySelectorAll('[data-tauri-drag-region]')).toHaveLength(1)
      for (const button of within(strip).getAllByRole('button')) {
        expect(button).not.toHaveAttribute('data-tauri-drag-region')
      }
      expect(screen.queryByTestId('traffic-light-clearance')).toBeNull()
    })

    it('adds a 24px clearance before the toggles on macOS', () => {
      renderPanel(mac)
      const clearance = screen.getByTestId('traffic-light-clearance')
      expect(clearance).toHaveStyle({ width: '24px' })
      expect(clearance).toHaveAttribute('data-tauri-drag-region')
      const strip = screen.getByTestId('top-row-toggle-strip')
      expect(strip.firstElementChild).toBe(clearance)
    })

    it('has no drag region on web', () => {
      renderPanel({ dragRegions: false, windowControls: false, trafficLights: false })
      expect(document.querySelector('[data-tauri-drag-region]')).toBeNull()
      expect(screen.getByTestId('top-row-toggle-strip')).toBeInTheDocument()
    })
  })
})
