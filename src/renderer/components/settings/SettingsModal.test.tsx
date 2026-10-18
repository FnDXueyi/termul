import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsModal } from './SettingsModal'

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'getClientRects').mockImplementation(function (
    this: HTMLElement
  ) {
    if (this.closest('[hidden]')) return [] as unknown as DOMRectList
    return [new DOMRect(0, 0, 10, 10)] as unknown as DOMRectList
  })
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('SettingsModal Tab trap', () => {
  it('wraps Tab among visible controls and skips a hidden page', () => {
    render(
      <SettingsModal isOpen onClose={() => undefined} title="Preferences">
        <button type="button">First</button>
        <section hidden>
          <button type="button">Hidden</button>
        </section>
        <button type="button">Last</button>
      </SettingsModal>
    )

    const first = screen.getByRole('button', { name: 'First' })
    const last = screen.getByRole('button', { name: 'Last' })
    last.focus()
    fireEvent.keyDown(window, { key: 'Tab' })
    expect(first).toHaveFocus()

    fireEvent.keyDown(window, { key: 'Tab', shiftKey: true })
    expect(last).toHaveFocus()
  })
})
