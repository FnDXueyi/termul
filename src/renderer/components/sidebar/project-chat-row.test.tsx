import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ChatRowTrail } from './project-chat-row'

describe('ChatRowTrail', () => {
  it('shows the done dot with an announced "New activity"', () => {
    render(<ChatRowTrail liveState="done" lastActivityAt={0} />)
    expect(screen.getByText('New activity')).toHaveClass('sr-only')
  })

  it('shows the Needs you word and the running spinner', () => {
    const { rerender } = render(<ChatRowTrail liveState="needs-you" lastActivityAt={0} />)
    expect(screen.getByText('Needs you')).toBeInTheDocument()
    rerender(<ChatRowTrail liveState="running" lastActivityAt={0} />)
    expect(screen.getByRole('status', { name: 'Running' })).toBeInTheDocument()
  })
})
