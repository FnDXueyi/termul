import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import type { FilePathResolutionContext } from '@/lib/file-path-links'
import { AgentProse } from './chat-agent-prose'

const openUrlWithSystemBrowser = vi.fn(() => Promise.resolve({ success: true, data: undefined }))
const logFrontendError = vi.fn(() => Promise.resolve())

vi.mock('@/lib/api', () => ({
  openerApi: {
    openUrlWithSystemBrowser: (...args: unknown[]) => openUrlWithSystemBrowser(...args)
  }
}))

vi.mock('@/lib/log-api', () => ({
  logFrontendError: (...args: unknown[]) => logFrontendError(...args)
}))

const scrolled: Element[] = []
const originalScrollIntoView = Element.prototype.scrollIntoView

beforeEach(() => {
  openUrlWithSystemBrowser.mockClear()
  logFrontendError.mockClear()
  scrolled.length = 0
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this)
  }
})

afterEach(() => {
  Element.prototype.scrollIntoView = originalScrollIntoView
})

function renderProse(text: string, filePathContext?: FilePathResolutionContext) {
  return render(
    <TooltipProvider>
      <AgentProse text={text} streaming={false} reduced filePathContext={filePathContext} />
    </TooltipProvider>
  )
}

// Streamdown opens its link modal after an awaited onLinkCheck, so flush microtasks.
async function click(element: HTMLElement): Promise<void> {
  fireEvent.click(element)
  await act(async () => {})
}

const PATHS: [string, FilePathResolutionContext | undefined][] = [
  ['without filePathContext', undefined],
  ['with filePathContext', { cwd: '/project' }]
]

const FOOTNOTE = 'Read the note[^1].\n\n[^1]: The footnote body.\n'

describe.each(PATHS)('AgentProse same-document links %s', (_label, context) => {
  it('does nothing but log when the fragment has no target in the message', async () => {
    renderProse('See [jump](#setup) for details.', context)

    await click(screen.getByText('jump'))

    expect(screen.queryByText('Open external link?')).toBeNull()
    expect(openUrlWithSystemBrowser).not.toHaveBeenCalled()
    expect(scrolled).toEqual([])
    expect(logFrontendError).toHaveBeenCalledWith({
      level: 'info',
      source: 'ChatMessage.anchorLink',
      message: "No element with id 'setup' in this message; ignoring '#setup'"
    })
  })

  it('scrolls to the element whose id matches the fragment', async () => {
    renderProse('See [jump](#setup).\n\n<h2 id="setup">Setup</h2>\n', context)

    await click(screen.getByText('jump'))

    expect(screen.queryByText('Open external link?')).toBeNull()
    expect(openUrlWithSystemBrowser).not.toHaveBeenCalled()
    expect(scrolled.map((el) => `${el.tagName}#${el.id}`)).toEqual(['H2#setup'])
  })

  it('moves between a footnote reference and its note', async () => {
    renderProse(FOOTNOTE, context)

    await click(screen.getByText('1'))
    await click(screen.getByText('↩'))

    expect(screen.queryByText('Open external link?')).toBeNull()
    expect(openUrlWithSystemBrowser).not.toHaveBeenCalled()
    expect(scrolled.map((el) => el.id)).toEqual(['user-content-fn-1', 'user-content-fnref-1'])
  })

  it('scrolls within the clicked message when another message reuses the id', async () => {
    const view = render(
      <TooltipProvider>
        <div data-testid="first">
          <AgentProse text={FOOTNOTE} streaming={false} reduced filePathContext={context} />
        </div>
        <div data-testid="second">
          <AgentProse text={FOOTNOTE} streaming={false} reduced filePathContext={context} />
        </div>
      </TooltipProvider>
    )
    const second = view.getByTestId('second')

    await click(within(second).getByText('1'))

    expect(scrolled).toHaveLength(1)
    expect(second.contains(scrolled[0])).toBe(true)
  })

  it('still confirms an external link before opening the system browser', async () => {
    renderProse('Visit [docs](https://example.com/docs).', context)

    await click(screen.getByText('docs'))
    await click(screen.getByRole('button', { name: 'Open' }))

    expect(openUrlWithSystemBrowser).toHaveBeenCalledWith('https://example.com/docs')
    expect(scrolled).toEqual([])
  })
})
