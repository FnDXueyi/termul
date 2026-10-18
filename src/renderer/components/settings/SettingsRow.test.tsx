import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { SettingsRow, settingsDescriptionId } from './SettingsRow'

describe('SettingsRow', () => {
  it('gives the description an id from describes when the row has no htmlFor', () => {
    render(
      <SettingsRow label="Interface zoom" description="50% to 300%." describes="ui-zoom">
        <span aria-describedby={settingsDescriptionId('ui-zoom')}>zoom</span>
      </SettingsRow>
    )

    expect(screen.getByText('50% to 300%.')).toHaveAttribute('id', 'ui-zoom-description')
    expect(screen.getByText('zoom')).toHaveAttribute('aria-describedby', 'ui-zoom-description')
    expect(screen.queryByRole('label')).not.toBeInTheDocument()
  })
})
