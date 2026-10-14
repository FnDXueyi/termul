import type { GitCommit } from '@shared/types/ipc.types'
import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { getLog } = vi.hoisted(() => ({ getLog: vi.fn() }))

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }))
vi.mock('@/lib/git-api', () => ({ gitApi: { getLog } }))

import { useGitHistoryStore } from '@/stores/git-history-store'
import { findPaneContainingTab, gitHistoryTabId, useWorkspaceStore } from '@/stores/workspace-store'
import { HistoryRow } from './HistoryRow'

const CWD = '/work'

function commit(subject: string, hash: string): GitCommit {
  return {
    hash: hash.padEnd(40, '0'),
    shortHash: hash,
    parents: [],
    refs: [],
    author: 'Ada',
    date: new Date(Date.now() - 2 * 3_600_000).toISOString(),
    subject
  }
}

describe('HistoryRow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getLog.mockResolvedValue([])
    useGitHistoryStore.setState({ commits: {}, loading: {}, error: {}, scope: {}, details: {} })
    const tabId = gitHistoryTabId(CWD)
    const pane = findPaneContainingTab(useWorkspaceStore.getState().root, tabId)
    if (pane) useWorkspaceStore.getState().removeTab(tabId)
  })

  it('shows the last commit and the count', () => {
    useGitHistoryStore.setState({
      commits: { [CWD]: [commit('Close the plan', 'a1b2c3d'), commit('Init', 'b2c3d4e')] }
    })
    render(<HistoryRow cwd={CWD} isVisible />)

    expect(screen.getByText('2 commits')).toBeInTheDocument()
    expect(screen.getByText(/^Close the plan · /)).toBeInTheDocument()
    expect(getLog).not.toHaveBeenCalled()
  })

  it('loads the history on first reveal only when visible', () => {
    const { rerender } = render(<HistoryRow cwd={CWD} isVisible={false} />)
    expect(getLog).not.toHaveBeenCalled()
    rerender(<HistoryRow cwd={CWD} isVisible />)
    expect(getLog).toHaveBeenCalledTimes(1)
  })

  it('opens the Git History tab and then shows it as open', () => {
    useGitHistoryStore.setState({ commits: { [CWD]: [commit('Init', 'a1b2c3d')] } })
    render(<HistoryRow cwd={CWD} isVisible />)

    fireEvent.click(screen.getByRole('button', { name: 'Open History in the main panel' }))

    expect(
      findPaneContainingTab(useWorkspaceStore.getState().root, gitHistoryTabId(CWD))
    ).not.toBeNull()
    expect(
      screen.getByRole('button', { name: 'History, open in the main panel' })
    ).toBeInTheDocument()
  })
})
