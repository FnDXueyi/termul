import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { SettingsSearchEntry } from '@/lib/settings-search'
import {
  SETTINGS_TARGET_MS,
  type SettingsCategory,
  SettingsLayout,
  SettingsSection
} from './SettingsLayout'

// jsdom does not implement scrollIntoView; search jumps use it.
window.HTMLElement.prototype.scrollIntoView = vi.fn()

beforeEach(() => {
  ;(window.HTMLElement.prototype.scrollIntoView as ReturnType<typeof vi.fn>).mockClear()
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
    cb(0)
    return 0
  })
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

const categories: SettingsCategory[] = [
  { id: 'appearance', label: 'Appearance', group: 'Terminal', description: 'How terminals look.' },
  { id: 'shell', label: 'Shell', group: 'Terminal' },
  { id: 'mcp-servers', label: 'MCP servers', group: 'Agents' },
  { id: 'updates', label: 'Updates', group: 'App' }
]

const searchIndex: SettingsSearchEntry[] = [
  {
    categoryId: 'appearance',
    label: 'Font family',
    description: 'monospace font',
    anchorId: 'setting-font'
  },
  { categoryId: 'shell', label: 'Default shell', keywords: ['bash'], anchorId: 'setting-shell' },
  {
    categoryId: 'mcp-servers',
    label: 'MCP servers',
    description: 'Manage stdio, HTTP, and SSE servers.',
    keywords: ['model context protocol']
  },
  { categoryId: 'updates', label: 'Auto-update', description: 'check for updates' }
]

function Pages(): React.JSX.Element {
  return (
    <>
      <SettingsSection id="appearance">
        <div id="setting-font">Font row</div>
      </SettingsSection>
      <SettingsSection id="shell">
        <div id="setting-shell">Shell row</div>
      </SettingsSection>
      <SettingsSection id="mcp-servers">
        <p>MCP body</p>
      </SettingsSection>
      <SettingsSection id="updates">
        <p>Updates body</p>
      </SettingsSection>
    </>
  )
}

function renderLayout(props: Partial<React.ComponentProps<typeof SettingsLayout>> = {}) {
  return render(
    <SettingsLayout title="Settings" categories={categories} searchIndex={searchIndex} {...props}>
      <Pages />
    </SettingsLayout>
  )
}

const nav = () => screen.getByRole('navigation', { name: 'Settings pages' })

describe('SettingsLayout', () => {
  it('lists every page in the sidebar under its group heading', () => {
    renderLayout()

    for (const category of categories) {
      expect(within(nav()).getByRole('button', { name: category.label })).toBeInTheDocument()
    }
    expect(screen.getAllByTestId('settings-group-label').map((el) => el.textContent)).toEqual([
      'Terminal',
      'Agents',
      'App'
    ])
  })

  it('shows only the first page by default, with its title and description', () => {
    renderLayout()

    expect(within(nav()).getByRole('button', { name: 'Appearance' })).toHaveAttribute(
      'aria-current',
      'page'
    )
    expect(screen.getByRole('heading', { name: 'Appearance', level: 2 })).toBeInTheDocument()
    expect(screen.getByText('How terminals look.')).toBeInTheDocument()
    expect(screen.getByText('Font row')).toBeVisible()
    expect(screen.getByText('Updates body')).not.toBeVisible()
  })

  it('switches the page when a sidebar item is selected, keeping other pages mounted', () => {
    renderLayout()

    fireEvent.click(within(nav()).getByRole('button', { name: 'Shell' }))

    expect(within(nav()).getByRole('button', { name: 'Shell' })).toHaveAttribute(
      'aria-current',
      'page'
    )
    expect(screen.getByText('Shell row')).toBeVisible()
    expect(screen.getByText('Font row')).not.toBeVisible()
    expect(screen.getByText('Font row')).toBeInTheDocument()
  })

  it('works controlled through page and onPageChange', () => {
    const onPageChange = vi.fn()
    function Controlled(): React.JSX.Element {
      const [page, setPage] = useState('updates')
      return (
        <SettingsLayout
          title="Settings"
          categories={categories}
          searchIndex={searchIndex}
          page={page}
          onPageChange={(next) => {
            onPageChange(next)
            setPage(next)
          }}
        >
          <Pages />
        </SettingsLayout>
      )
    }
    render(<Controlled />)

    expect(screen.getByText('Updates body')).toBeVisible()
    fireEvent.click(within(nav()).getByRole('button', { name: 'MCP servers' }))
    expect(onPageChange).toHaveBeenCalledWith('mcp-servers')
    expect(screen.getByText('MCP body')).toBeVisible()
  })

  it('filters to matching settings with the page path, and matches keywords', () => {
    renderLayout()

    fireEvent.change(screen.getByRole('textbox', { name: /search settings/i }), {
      target: { value: 'bash' }
    })

    const result = within(nav()).getByRole('button', { name: /Default shell/ })
    expect(result).toHaveTextContent('Terminal · Shell')
    expect(screen.getByText('1 result')).toBeInTheDocument()
  })

  it('shows an empty state with Clear search when no settings match', () => {
    renderLayout()

    fireEvent.change(screen.getByRole('textbox', { name: /search settings/i }), {
      target: { value: 'zzzzqqq' }
    })
    expect(screen.getByText(/No settings match/i)).toBeInTheDocument()

    fireEvent.click(within(nav()).getByRole('button', { name: 'Clear search' }))
    expect(screen.getByRole('textbox', { name: /search settings/i })).toHaveValue('')
  })

  it('opens the page of a result, scrolls to the setting and marks it for two seconds', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    renderLayout()

    fireEvent.change(screen.getByRole('textbox', { name: /search settings/i }), {
      target: { value: 'bash' }
    })
    fireEvent.click(within(nav()).getByRole('button', { name: /Default shell/ }))

    const target = screen.getByText('Shell row')
    expect(target).toBeVisible()
    expect(window.HTMLElement.prototype.scrollIntoView).toHaveBeenCalled()
    expect(target).toHaveAttribute('data-settings-target')

    act(() => {
      vi.advanceTimersByTime(SETTINGS_TARGET_MS)
    })
    expect(target).not.toHaveAttribute('data-settings-target')
  })

  it('cancels a pending jump so the first row does not stay marked', () => {
    const frames = new Map<number, FrameRequestCallback>()
    let nextId = 1
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      const id = nextId++
      frames.set(id, cb)
      return id
    })
    vi.stubGlobal('cancelAnimationFrame', (id: number) => {
      frames.delete(id)
    })
    renderLayout()

    fireEvent.change(screen.getByRole('textbox', { name: /search settings/i }), {
      target: { value: 'bash' }
    })
    fireEvent.click(within(nav()).getByRole('button', { name: /Default shell/ }))
    fireEvent.change(screen.getByRole('textbox', { name: /search settings/i }), {
      target: { value: 'font' }
    })
    fireEvent.click(within(nav()).getByRole('button', { name: /Font family/ }))

    expect(frames.size).toBe(1)
    act(() => {
      for (const frame of frames.values()) frame(0)
    })

    expect(screen.getByText('Shell row')).not.toHaveAttribute('data-settings-target')
    expect(screen.getByText('Font row')).toHaveAttribute('data-settings-target')
  })

  it('cancels a pending jump when the layout unmounts', () => {
    const frames = new Map<number, FrameRequestCallback>()
    let nextId = 1
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      const id = nextId++
      frames.set(id, cb)
      return id
    })
    vi.stubGlobal('cancelAnimationFrame', (id: number) => {
      frames.delete(id)
    })
    const view = renderLayout()

    fireEvent.change(screen.getByRole('textbox', { name: /search settings/i }), {
      target: { value: 'bash' }
    })
    fireEvent.click(within(nav()).getByRole('button', { name: /Default shell/ }))
    expect(frames.size).toBe(1)

    view.unmount()
    expect(frames.size).toBe(0)
  })

  it('clears the search when the clear button is pressed', () => {
    renderLayout()

    const input = screen.getByRole('textbox', { name: /search settings/i })
    fireEvent.change(input, { target: { value: 'font' } })
    fireEvent.click(screen.getAllByRole('button', { name: /clear search/i })[0])

    expect(input).toHaveValue('')
    expect(within(nav()).getByRole('button', { name: 'Appearance' })).toBeInTheDocument()
  })

  it('moves focus between pages with the arrow keys', () => {
    renderLayout()

    const first = within(nav()).getByRole('button', { name: 'Appearance' })
    first.focus()
    fireEvent.keyDown(first, { key: 'ArrowDown' })
    expect(within(nav()).getByRole('button', { name: 'Shell' })).toHaveFocus()
  })

  it('renders a named Close button when onClose is given', () => {
    const onClose = vi.fn()
    renderLayout({ onClose })

    const [close] = screen.getAllByRole('button', { name: 'Close Settings' })
    fireEvent.click(close)
    expect(onClose).toHaveBeenCalled()
  })

  it('marks the active page with a neutral keycap; idle rows hover at 3%', () => {
    renderLayout()

    const active = within(nav()).getByRole('button', { name: 'Appearance' })
    const idle = within(nav()).getByRole('button', { name: 'Shell' })
    expect(active.className).toContain('keycap')
    expect(active.className).not.toContain('bg-primary')
    expect(idle.className).toContain('hover:bg-foreground/[0.03]')
  })
})
