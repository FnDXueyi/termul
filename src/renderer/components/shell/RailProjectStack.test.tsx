import { fireEvent, render, screen, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ProjectRowStatus } from '@/components/sidebar/indicators'
import { TooltipProvider } from '@/components/ui/tooltip'
import type { Project } from '@/types/project'
import { pickShownProjects, RAIL_STACK_OTHERS, RailProjectStack } from './RailProjectStack'

const { statusRef, navigateSpy } = vi.hoisted(() => ({
  statusRef: { current: {} as Record<string, Partial<ProjectRowStatus>> },
  navigateSpy: vi.fn()
}))

vi.mock('@/components/sidebar/use-project-row-status', () => ({
  useProjectRowStatus: () => (id: string) => ({
    live: null,
    attentionCount: 0,
    crashed: false,
    ...statusRef.current[id]
  })
}))
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return { ...actual, useNavigate: () => navigateSpy }
})

const project = (id: string, extra: Partial<Project> = {}): Project =>
  ({ id, name: id.toUpperCase(), color: 'blue', path: `/repo/${id}`, ...extra }) as Project

function renderStack(projects: Project[], activeId = 'a') {
  const onSelectProject = vi.fn()
  render(
    <MemoryRouter>
      <TooltipProvider>
        <RailProjectStack
          projects={projects}
          activeProject={projects.find((p) => p.id === activeId)}
          onSelectProject={onSelectProject}
          onNewProject={vi.fn()}
        />
      </TooltipProvider>
    </MemoryRouter>
  )
  const stack = screen.getByTestId('rail-project-stack')
  const names = within(stack)
    .getAllByRole('button')
    .map((button) => button.getAttribute('aria-label'))
  return { onSelectProject, names }
}

beforeEach(() => {
  vi.clearAllMocks()
  statusRef.current = {}
})

describe('RailProjectStack', () => {
  it('puts the active project first on the keycap, then the other open projects', () => {
    const { names } = renderStack([project('a'), project('b'), project('z', { isArchived: true })])
    expect(names).toEqual(['Switch project (A)', 'Open B'])
    expect(screen.getByRole('button', { name: 'Switch project (A)' })).toHaveClass('keycap')
  })

  it('switches project and goes to the workspace from an icon', () => {
    const { onSelectProject } = renderStack([project('a'), project('b')])
    fireEvent.click(screen.getByRole('button', { name: 'Open B' }))
    expect(onSelectProject).toHaveBeenCalledWith('b')
    expect(navigateSpy).toHaveBeenCalledWith('/')
  })

  it('keeps list order when statuses change, so no icon moves under the pointer', () => {
    statusRef.current = {
      c: { live: 'activity' },
      d: { crashed: true },
      e: { attentionCount: 2 }
    }
    const { names } = renderStack([
      project('a'),
      project('b'),
      project('c'),
      project('d'),
      project('e')
    ])
    expect(names).toEqual([
      'Switch project (A)',
      'Open B',
      'Open C, Running',
      'Open D, Terminal crashed',
      'Open E, 2 need you'
    ])
  })

  it(`shows ${RAIL_STACK_OTHERS} others and puts the rest behind "+N"`, () => {
    const many = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map((id) => project(id))
    const { names } = renderStack(many)
    expect(names).toHaveLength(1 + RAIL_STACK_OTHERS + 1)
    expect(names.at(-1)).toBe('2 more projects')
    expect(screen.getByRole('button', { name: '2 more projects' })).toHaveTextContent('+2')
  })

  it('puts a hidden project that needs you in the last visible slot', () => {
    statusRef.current = { g: { attentionCount: 1 } }
    const many = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map((id) => project(id))
    const { names } = renderStack(many)
    expect(names).toEqual([
      'Switch project (A)',
      'Open B',
      'Open C',
      'Open D',
      'Open G, 1 needs you',
      '2 more projects'
    ])
  })
})

describe('pickShownProjects', () => {
  const item = (id: string, attentionCount = 0) => ({
    id,
    status: { live: null, attentionCount, crashed: false } as ProjectRowStatus
  })

  it('keeps the first ones in order when nothing hidden needs you', () => {
    const others = ['b', 'c', 'd'].map((id) => item(id))
    expect(pickShownProjects(others, 2).map((o) => o.id)).toEqual(['b', 'c'])
  })

  it('does not swap when a shown project already needs you', () => {
    const others = [item('b', 1), item('c'), item('d', 3)]
    expect(pickShownProjects(others, 2).map((o) => o.id)).toEqual(['b', 'c'])
  })

  it('swaps the first hidden project that needs you into the last slot', () => {
    const others = [item('b'), item('c'), item('d'), item('e', 2), item('f', 1)]
    expect(pickShownProjects(others, 3).map((o) => o.id)).toEqual(['b', 'c', 'e'])
  })
})
