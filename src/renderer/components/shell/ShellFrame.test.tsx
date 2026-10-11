import { act, render, renderHook, screen } from '@testing-library/react'
import { useEffect } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  resetNativeWebviewOcclusionForTests,
  useNativeWebviewOccluded
} from '@/hooks/use-native-webview-occlusion'
import type { SSHWorkspaceState } from '@/layouts/workspace-layout/use-ssh-workspace'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import { useThemePickerStore } from '@/stores/theme-picker-store'
import { ShellFrame, type ShellFrameProps } from './ShellFrame'
import type { ShellRailProps } from './ShellRail'

vi.mock('@/lib/tauri-runtime', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-runtime')>('@/lib/tauri-runtime')
  return { ...actual, isTauriContext: () => true }
})

vi.mock('@/components/ActivityRail', () => ({
  ActivityRail: () => <div data-testid="activity-rail" />
}))
vi.mock('@/components/ResizeEdges', () => ({ ResizeEdges: () => null }))
vi.mock('@/components/TitleBar', () => ({ TitleBar: () => <div data-testid="title-bar" /> }))
vi.mock('@/layouts/workspace-layout/MacOsTitlebarStrip', () => ({
  MacOsTitlebarStrip: () => null
}))
vi.mock('@/components/workspace/WorkspaceConflictBanner', () => ({
  WorkspaceConflictBanner: () => <div data-testid="conflict-banner" />
}))
vi.mock('@/hooks/use-pane-dnd', () => ({
  PaneDndProvider: ({ children }: { children: React.ReactNode }) => (
    <div data-testid="pane-dnd">{children}</div>
  )
}))
const railProps: { current: ShellRailProps | null } = { current: null }
vi.mock('@/components/shell/ShellRail', () => ({
  ShellRail: (props: ShellRailProps) => {
    railProps.current = props
    return <div data-testid="rail" />
  }
}))
vi.mock('@/layouts/workspace-layout/ExplorerColumn', () => ({
  ExplorerColumn: ({ side }: { side?: string }) => <div data-testid="side-panel" data-side={side} />
}))

const mountSpy = vi.fn()
const unmountSpy = vi.fn()

function WorkspaceMainProbe(): React.JSX.Element {
  useEffect(() => {
    mountSpy()
    return () => unmountSpy()
  }, [])
  return <div data-testid="workspace-main" />
}

const ssh = {
  activeSSHProfileId: null,
  handleSSHConnect: vi.fn(),
  handleSelectSSHProfile: vi.fn()
} as unknown as SSHWorkspaceState

function buildProps(overrides: Partial<ShellFrameProps> = {}): ShellFrameProps {
  return {
    projects: [],
    activeProject: undefined,
    activeProjectId: '',
    ssh,
    isShortcutMenuOpen: false,
    setIsShortcutMenuOpen: vi.fn(),
    setIsCommandPaletteOpen: vi.fn(),
    setIsNewProjectModalOpen: vi.fn(),
    onSelectProject: vi.fn(),
    handleAddGitTab: vi.fn(),
    handleAddGitHistoryTab: vi.fn(),
    handleToggleThemePicker: vi.fn(),
    handleOpenAgentChat: vi.fn(),
    handleOpenCanvas: vi.fn(),
    workspaceMain: <WorkspaceMainProbe />,
    appModals: <div data-testid="app-modals" />,
    ...overrides
  }
}

describe('ShellFrame', () => {
  beforeEach(() => {
    mountSpy.mockClear()
    unmountSpy.mockClear()
    resetNativeWebviewOcclusionForTests()
    resetShellLayoutStoreForTests()
    useShellLayoutStore.setState({ status: 'unavailable' })
    useThemePickerStore.getState().close()
    railProps.current = null
  })

  afterEach(() => {
    resetNativeWebviewOcclusionForTests()
    useThemePickerStore.getState().close()
  })

  it('renders the Rail, then the Side panel on the left of the main area', () => {
    render(<ShellFrame {...buildProps()} />)

    const rail = screen.getByTestId('rail')
    const sidePanel = screen.getByTestId('side-panel')
    const main = screen.getByTestId('workspace-main').closest('main')
    expect(main).not.toBeNull()
    expect(sidePanel).toHaveAttribute('data-side', 'left')
    // DOM order: Rail, Side panel, main.
    expect(rail.compareDocumentPosition(sidePanel) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(
      sidePanel.compareDocumentPosition(main as Node) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
    // Side panel and main share the drag-and-drop provider; the rail does not.
    const dnd = screen.getByTestId('pane-dnd')
    expect(dnd).toContainElement(sidePanel)
    expect(dnd).toContainElement(main)
    expect(dnd).not.toContainElement(rail)
    // The Rail replaces the classic activity rail.
    expect(screen.queryByTestId('activity-rail')).not.toBeInTheDocument()
    expect(screen.getByTestId('title-bar')).toBeInTheDocument()
    expect(screen.getByTestId('conflict-banner')).toBeInTheDocument()
    expect(screen.getByTestId('app-modals')).toBeInTheDocument()
  })

  it('wires the Rail: palette, canvas, shortcuts, theme picker and project props', () => {
    const setIsCommandPaletteOpen = vi.fn()
    const setIsShortcutMenuOpen = vi.fn()
    const handleOpenCanvas = vi.fn()
    const handleToggleThemePicker = vi.fn()
    const onSelectProject = vi.fn()
    const activeProject = { id: 'p1', name: 'One', color: 'blue', path: '/repo/one' } as const
    render(
      <ShellFrame
        {...buildProps({
          projects: [activeProject],
          activeProject,
          activeProjectId: 'p1',
          isShortcutMenuOpen: true,
          setIsCommandPaletteOpen,
          setIsShortcutMenuOpen,
          handleOpenCanvas,
          handleToggleThemePicker,
          onSelectProject
        })}
      />
    )

    const props = railProps.current as ShellRailProps
    expect(props.projects).toEqual([activeProject])
    expect(props.activeProject).toBe(activeProject)
    expect(props.activeProjectId).toBe('p1')
    expect(props.canOpenCanvas).toBe(true)
    expect(props.isShortcutsOpen).toBe(true)
    expect(props.isThemePickerOpen).toBe(false)

    props.onOpenCommandPalette()
    expect(setIsCommandPaletteOpen).toHaveBeenCalledWith(true)
    props.onOpenCanvas()
    expect(handleOpenCanvas).toHaveBeenCalledTimes(1)
    props.onShortcutsOpenChange(false)
    expect(setIsShortcutMenuOpen).toHaveBeenCalledWith(false)
    props.onToggleThemePicker?.()
    expect(handleToggleThemePicker).toHaveBeenCalledTimes(1)
    props.onSelectProject('p1')
    expect(onSelectProject).toHaveBeenCalledWith('p1')
  })

  it('cannot open the canvas without a project path', () => {
    render(<ShellFrame {...buildProps()} />)
    expect((railProps.current as ShellRailProps).canOpenCanvas).toBe(false)
  })

  it('does not remount workspaceMain when the Rail flips between expanded and collapsed', () => {
    render(<ShellFrame {...buildProps()} />)
    expect(mountSpy).toHaveBeenCalledTimes(1)

    act(() => useShellLayoutStore.setState({ railExpanded: false }))
    act(() => useShellLayoutStore.setState({ railExpanded: true }))
    act(() => useShellLayoutStore.setState({ railWidth: 280 }))

    expect(mountSpy).toHaveBeenCalledTimes(1)
    expect(unmountSpy).not.toHaveBeenCalled()
  })

  it('does not remount workspaceMain when the frame re-renders', () => {
    const { rerender } = render(<ShellFrame {...buildProps()} />)
    expect(mountSpy).toHaveBeenCalledTimes(1)

    rerender(<ShellFrame {...buildProps({ isShortcutMenuOpen: true })} />)
    rerender(<ShellFrame {...buildProps({ isShortcutMenuOpen: false })} />)

    expect(mountSpy).toHaveBeenCalledTimes(1)
    expect(unmountSpy).not.toHaveBeenCalled()
  })

  it('occludes native webviews while the shortcuts menu is open', () => {
    const observer = renderHook(() => useNativeWebviewOccluded())
    const { rerender } = render(<ShellFrame {...buildProps()} />)
    expect(observer.result.current).toBe(false)

    rerender(<ShellFrame {...buildProps({ isShortcutMenuOpen: true })} />)
    expect(observer.result.current).toBe(true)

    rerender(<ShellFrame {...buildProps({ isShortcutMenuOpen: false })} />)
    expect(observer.result.current).toBe(false)
  })

  it('occludes native webviews while the theme picker is open, until both occluders close', () => {
    const observer = renderHook(() => useNativeWebviewOccluded())
    const { rerender } = render(<ShellFrame {...buildProps({ isShortcutMenuOpen: true })} />)

    act(() => useThemePickerStore.getState().open('termul'))
    expect(observer.result.current).toBe(true)

    // Shortcuts closes first: the theme picker still occludes.
    rerender(<ShellFrame {...buildProps({ isShortcutMenuOpen: false })} />)
    expect(observer.result.current).toBe(true)

    act(() => useThemePickerStore.getState().close())
    expect(observer.result.current).toBe(false)
  })
})
