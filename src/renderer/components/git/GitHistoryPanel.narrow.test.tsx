import type { GitCommit } from '@shared/types/ipc.types'
import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { getLog, mobileRef } = vi.hoisted(() => ({
  getLog: vi.fn(),
  mobileRef: { current: false as boolean }
}))

vi.mock('sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn() }
}))
vi.mock('@/lib/git-api', () => ({ gitApi: { getLog } }))
vi.mock('@/hooks/use-mobile-web-shell', () => ({
  useMobileWebShell: () => mobileRef.current,
  MOBILE_WEB_SHELL_MAX_PX: 767
}))

import { useGitHistoryStore } from '@/stores/git-history-store'
import { GitHistoryPanel } from './GitHistoryPanel'

const CWD = '/work'
const fullHash = (short: string): string => short.padEnd(40, '0')

const COMMITS: GitCommit[] = [
  {
    hash: fullHash('a1b2c3d'),
    shortHash: 'a1b2c3d',
    parents: [fullHash('d4e5f6a')],
    refs: ['HEAD -> refs/heads/main'],
    author: 'Ada Lovelace',
    date: new Date().toISOString(),
    subject: 'Add login form'
  },
  {
    hash: fullHash('d4e5f6a'),
    shortHash: 'd4e5f6a',
    parents: [],
    refs: [],
    author: 'Grace Hopper',
    date: new Date().toISOString(),
    subject: 'Initial commit'
  }
]

function rowFor(commit: GitCommit): HTMLElement {
  return screen.getByTitle(`${commit.shortHash} — ${commit.subject}`)
}

describe('GitHistoryPanel narrow variant', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getLog.mockResolvedValue([])
    mobileRef.current = false
    useGitHistoryStore.setState({
      commits: { [CWD]: COMMITS },
      loading: {},
      error: {}
    })
  })

  it('renders the two-line phone rows at the 44px pitch under the desktop header', () => {
    const { container } = render(<GitHistoryPanel cwd={CWD} isVisible variant="narrow" />)

    for (const commit of COMMITS) {
      const row = rowFor(commit)
      expect(row.style.height).toBe('44px')
      expect(row.className).toContain('flex-col')
    }
    // Desktop-sized header controls, filter below the title line.
    expect(screen.getByLabelText('Filter commits').className).toContain('w-full')
    expect(screen.getByRole('button', { name: 'Refresh history' }).className).toContain('h-8 w-8')
    // Phone lane geometry: 8px padding + 12px lane width.
    const svg = container.querySelector('svg.left-0.top-0')
    expect(svg?.getAttribute('width')).toBe(String(8 * 2 + 12))
    // No toggle handler: the title is plain text, not a button.
    expect(screen.queryByRole('button', { name: 'Git History' })).toBeNull()
  })

  it('collapses to the header alone through the title button', () => {
    const onToggle = vi.fn()
    const { rerender } = render(
      <GitHistoryPanel cwd={CWD} isVisible variant="narrow" onToggleCollapsed={onToggle} />
    )

    const title = screen.getByRole('button', { name: 'Git History' })
    expect(title).toHaveAttribute('aria-expanded', 'true')
    fireEvent.click(title)
    expect(onToggle).toHaveBeenCalledTimes(1)

    rerender(
      <GitHistoryPanel
        cwd={CWD}
        isVisible
        variant="narrow"
        collapsed
        onToggleCollapsed={onToggle}
      />
    )
    expect(screen.getByRole('button', { name: 'Git History' })).toHaveAttribute(
      'aria-expanded',
      'false'
    )
    expect(screen.queryByLabelText('Filter commits')).toBeNull()
    expect(screen.queryByTitle(/Add login form/)).toBeNull()
    expect(screen.getByRole('button', { name: 'Refresh history' })).toBeInTheDocument()
  })

  it('shows the empty state in the body when there is no history', () => {
    useGitHistoryStore.setState({ commits: { [CWD]: [] }, loading: {}, error: { [CWD]: 'boom' } })
    render(<GitHistoryPanel cwd={CWD} isVisible variant="narrow" />)

    expect(screen.getByText('No commit history')).toBeInTheDocument()
    expect(screen.getByText(/may not be a Git repository/)).toBeInTheDocument()
  })

  it('keeps the auto variant on the desktop rows', () => {
    render(<GitHistoryPanel cwd={CWD} isVisible />)

    expect(rowFor(COMMITS[0]).style.height).toBe('30px')
    expect(screen.getByLabelText('Filter commits').className).toContain('w-44')
  })

  it('keeps the auto variant on the phone rows on the phone shell', () => {
    mobileRef.current = true
    render(<GitHistoryPanel cwd={CWD} isVisible />)

    expect(rowFor(COMMITS[0]).style.height).toBe('44px')
    expect(screen.getByLabelText('Filter commits').className).toContain('h-11')
  })
})
