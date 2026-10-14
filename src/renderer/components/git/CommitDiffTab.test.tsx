import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { getCommitFileDiff } = vi.hoisted(() => ({ getCommitFileDiff: vi.fn() }))

vi.mock('@/lib/git-api', () => ({ gitApi: { getCommitFileDiff } }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { CommitDiffTab } from './CommitDiffTab'

const HASH = 'e44b917'.padEnd(40, '0')
const DIFF = [
  'diff --git a/src/a.ts b/src/a.ts',
  '--- a/src/a.ts',
  '+++ b/src/a.ts',
  '@@ -1,3 +1,3 @@',
  ' const x = 1',
  '-oldLine',
  '+newLine',
  ' const y = 2',
  ''
].join('\n')

describe('CommitDiffTab', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('loads once when shown and labels the side with the short hash', async () => {
    getCommitFileDiff.mockResolvedValue(DIFF)
    const { rerender } = render(
      <CommitDiffTab cwd="/r" commit={HASH} path="src/a.ts" isVisible={false} />
    )
    expect(getCommitFileDiff).not.toHaveBeenCalled()

    rerender(<CommitDiffTab cwd="/r" commit={HASH} path="src/a.ts" isVisible />)
    expect(await screen.findByText('at e44b917')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText(/newLine/)).toBeInTheDocument())
    expect(getCommitFileDiff).toHaveBeenCalledWith('/r', HASH, 'src/a.ts', undefined)

    rerender(<CommitDiffTab cwd="/r" commit={HASH} path="src/a.ts" isVisible={false} />)
    rerender(<CommitDiffTab cwd="/r" commit={HASH} path="src/a.ts" isVisible />)
    expect(getCommitFileDiff).toHaveBeenCalledTimes(1)
  })

  it('has no hunk stage actions (read-only)', async () => {
    getCommitFileDiff.mockResolvedValue(DIFF)
    render(<CommitDiffTab cwd="/r" commit={HASH} path="src/a.ts" isVisible />)
    await waitFor(() => expect(screen.getByText(/newLine/)).toBeInTheDocument())
    expect(screen.queryByRole('button', { name: /stage/i })).toBeNull()
  })

  it('shows a Retry state on failure and loads again', async () => {
    getCommitFileDiff.mockRejectedValueOnce(new Error('bad revision')).mockResolvedValueOnce(DIFF)
    render(<CommitDiffTab cwd="/r" commit={HASH} path="src/a.ts" oldPath="src/old.ts" isVisible />)
    expect(await screen.findByText('bad revision')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }))
    await waitFor(() => expect(screen.getByText(/newLine/)).toBeInTheDocument())
    expect(getCommitFileDiff).toHaveBeenLastCalledWith('/r', HASH, 'src/a.ts', 'src/old.ts')
  })
})
