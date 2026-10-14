import type { GitCommit, GitCommitDetail } from '@shared/types/ipc.types'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { getLog, getCommitDetail, writeText, addCommitDiffTab } = vi.hoisted(() => ({
  getLog: vi.fn(),
  getCommitDetail: vi.fn(),
  writeText: vi.fn(),
  addCommitDiffTab: vi.fn()
}))

vi.mock('sonner', () => {
  const toast = Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() })
  return { toast }
})
vi.mock('@/lib/git-api', () => ({ gitApi: { getLog, getCommitDetail } }))
vi.mock('@/lib/clipboard-api', () => ({ clipboardApi: { writeText } }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { useGitHistoryStore } from '@/stores/git-history-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import { GitHistoryView } from './GitHistoryView'

const CWD = '/repo'
const full = (short: string): string => short.padEnd(40, '0')

function commit(short: string, subject: string, author: string, parents: string[] = []): GitCommit {
  return {
    hash: full(short),
    shortHash: short,
    parents: parents.map(full),
    refs: [],
    author,
    date: new Date(Date.now() - 3_600_000).toISOString(),
    subject
  }
}

const COMMITS: GitCommit[] = [
  { ...commit('c3c3c3c', 'Add QRIS payment', 'Ada', ['b2b2b2b']), refs: ['HEAD -> main'] },
  commit('b2b2b2b', 'Fix keypad typo', 'Grace', ['a1a1a1a']),
  commit('a1a1a1a', 'Initial commit', 'Ada')
]

function detailFor(hash: string): GitCommitDetail {
  return {
    hash,
    authorEmail: 'ada@example.com',
    message: 'Add QRIS payment\n\nThe cashier sees the QR code.',
    parents: [full('b2b2b2b')],
    files: [
      { path: 'app/src/payments/qris.ts', oldPath: null, status: 'A', added: 64, removed: 0 },
      { path: 'app/src/ui/Keypad.tsx', oldPath: null, status: 'M', added: 12, removed: 9 }
    ],
    filesTruncated: false
  }
}

function renderView() {
  return render(<GitHistoryView cwd={CWD} isVisible />)
}

function rows(): HTMLElement[] {
  return within(screen.getByRole('listbox', { name: 'Commits' })).getAllByRole('option')
}

function selectedRow(): HTMLElement | undefined {
  return rows().find((r) => r.getAttribute('aria-selected') === 'true')
}

describe('GitHistoryView', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    getLog.mockResolvedValue(COMMITS)
    getCommitDetail.mockImplementation(async (_cwd: string, hash: string) => detailFor(hash))
    writeText.mockResolvedValue({ success: true, data: undefined })
    useGitHistoryStore.setState({
      commits: { [CWD]: COMMITS },
      loading: {},
      error: {},
      scope: {},
      details: {}
    })
    useWorkspaceStore.setState({ addCommitDiffTab })
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      cb(0)
      return 0
    })
    Element.prototype.scrollIntoView = vi.fn()
  })

  it('renders one row per commit and selects the newest commit', () => {
    renderView()
    expect(rows()).toHaveLength(3)
    expect(selectedRow()).toHaveTextContent('Add QRIS payment')
    expect(screen.getByText('3 commits')).toBeInTheDocument()
  })

  it('shows the commit detail with message body, email, parents and files', async () => {
    renderView()
    expect(await screen.findByText('qris.ts')).toBeInTheDocument()
    expect(screen.getByText('The cashier sees the QR code.')).toBeInTheDocument()
    expect(screen.getByText(/ada@example\.com/)).toBeInTheDocument()
    expect(screen.getByText('2 files changed')).toBeInTheDocument()
    expect(getCommitDetail).toHaveBeenCalledWith(CWD, full('c3c3c3c'))
  })

  it('opens a read-only commit diff tab when a changed file is clicked', async () => {
    renderView()
    fireEvent.click(await screen.findByText('Keypad.tsx'))
    expect(addCommitDiffTab).toHaveBeenCalledWith(
      CWD,
      full('c3c3c3c'),
      'app/src/ui/Keypad.tsx',
      undefined
    )
  })

  it('moves the selection with the arrow keys and opens the first file on Enter', async () => {
    renderView()
    const list = screen.getByRole('listbox', { name: 'Commits' })
    fireEvent.keyDown(list, { key: 'ArrowDown' })
    expect(selectedRow()).toHaveTextContent('Fix keypad typo')
    fireEvent.keyDown(list, { key: 'ArrowUp' })
    expect(selectedRow()).toHaveTextContent('Add QRIS payment')

    await screen.findByText('qris.ts')
    fireEvent.keyDown(list, { key: 'Enter' })
    expect(addCommitDiffTab).toHaveBeenCalledWith(
      CWD,
      full('c3c3c3c'),
      'app/src/payments/qris.ts',
      undefined
    )
  })

  it('copies the full hash with Ctrl+C', async () => {
    renderView()
    fireEvent.keyDown(screen.getByRole('listbox', { name: 'Commits' }), { key: 'c', ctrlKey: true })
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(full('c3c3c3c')))
  })

  it('hides the detail pane on Esc and shows it again from the toolbar', () => {
    renderView()
    expect(screen.getByRole('complementary', { name: 'Commit detail' })).toBeInTheDocument()
    fireEvent.keyDown(screen.getByRole('listbox', { name: 'Commits' }), { key: 'Escape' })
    expect(screen.queryByRole('complementary', { name: 'Commit detail' })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Show details' }))
    expect(screen.getByRole('complementary', { name: 'Commit detail' })).toBeInTheDocument()
  })

  it('focuses the filter on / and filters by message, author and hash', () => {
    renderView()
    fireEvent.keyDown(screen.getByRole('listbox', { name: 'Commits' }), { key: '/' })
    const filter = screen.getByLabelText('Filter commits')
    expect(document.activeElement).toBe(filter)

    fireEvent.change(filter, { target: { value: 'grace' } })
    expect(rows()).toHaveLength(1)
    expect(screen.getByText('1 of 3')).toBeInTheDocument()

    fireEvent.change(filter, { target: { value: 'a1a1a1a' } })
    expect(rows()[0]).toHaveTextContent('Initial commit')

    fireEvent.change(filter, { target: { value: 'zzz' } })
    expect(screen.getByText('No commits match this filter.')).toBeInTheDocument()

    // Esc in the filter clears it first.
    fireEvent.keyDown(filter, { key: 'Escape' })
    expect((filter as HTMLInputElement).value).toBe('')
    expect(rows()).toHaveLength(3)
  })

  it('hides the lane graph while filtering', () => {
    renderView()
    const graph = () =>
      screen.getByRole('listbox', { name: 'Commits' }).querySelector('svg.top-0.left-0')
    expect(graph()).not.toBeNull()
    fireEvent.change(screen.getByLabelText('Filter commits'), { target: { value: 'Ada' } })
    expect(graph()).toBeNull()
  })

  it('selects a parent commit from the detail pane', async () => {
    renderView()
    fireEvent.click(await screen.findByRole('button', { name: 'b2b2b2b' }))
    expect(selectedRow()).toHaveTextContent('Fix keypad typo')
  })

  it('reloads with --all off when the scope changes to the current branch', async () => {
    renderView()
    await act(async () => {
      useGitHistoryStore.getState().setScope(CWD, 'head')
    })
    expect(getLog).toHaveBeenCalledWith(CWD, undefined, false)
    expect(screen.getByRole('button', { name: 'Scope: Current branch' })).toBeInTheDocument()
  })

  it('shows the empty state with no commits', () => {
    useGitHistoryStore.setState({ commits: { [CWD]: [] } })
    renderView()
    expect(screen.getByText('No commit history')).toBeInTheDocument()
  })
})
