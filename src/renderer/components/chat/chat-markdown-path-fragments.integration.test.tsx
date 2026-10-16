import { fireEvent, render, waitFor } from '@testing-library/react'
import { type Components, defaultRemarkPlugins, Streamdown } from 'streamdown'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  getFileInfo: vi.fn(),
  openFile: vi.fn(),
  updateCursorPosition: vi.fn(),
  addEditorTab: vi.fn(),
  toastError: vi.fn(),
  openFiles: new Map<string, unknown>()
}))

vi.mock('@/lib/api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api')>()
  return {
    ...actual,
    filesystemApi: { ...actual.filesystemApi, getFileInfo: mocks.getFileInfo }
  }
})

vi.mock('@/stores/editor-store', () => ({
  useEditorStore: {
    getState: () => ({
      openFiles: mocks.openFiles,
      openFile: mocks.openFile,
      updateCursorPosition: mocks.updateCursorPosition
    })
  }
}))

vi.mock('@/stores/workspace-store', () => ({
  useWorkspaceStore: { getState: () => ({ addEditorTab: mocks.addEditorTab }) }
}))

vi.mock('sonner', () => ({ toast: { error: mocks.toastError } }))

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn(() => Promise.resolve()) }))

import { TermulFilePathButton } from './chat-agent-prose'
import { ChatMarkdownCode } from './chat-markdown-code'
import { remarkFilePathLinks } from './chat-markdown-file-links'

const CWD = '/repo'

/** Mirrors the AgentProse file-path wiring with real Streamdown and the real opener. */
function renderChat(text: string): HTMLElement {
  const components: Components = {
    code: ChatMarkdownCode,
    'termul-file-path': (props: Record<string, unknown>) => (
      <TermulFilePathButton
        path={typeof props['data-path'] === 'string' ? props['data-path'] : ''}
        context={{ cwd: CWD }}
      >
        {props.children as React.ReactNode}
      </TermulFilePathButton>
    )
  }
  const { container } = render(
    <Streamdown
      mode="static"
      remarkPlugins={[...Object.values(defaultRemarkPlugins), remarkFilePathLinks]}
      allowedTags={{ 'termul-file-path': ['dataPath'] }}
      components={components}
      controls={false}
      lineNumbers={false}
      linkSafety={{ enabled: false }}
    >
      {text}
    </Streamdown>
  )
  return container
}

describe('agent prose path links with line fragments and inline-code filenames', () => {
  beforeEach(() => {
    mocks.getFileInfo.mockReset().mockImplementation(async (path: string) => ({
      success: true,
      data: { path, size: 1, modifiedAt: 1, type: 'file', isReadOnly: false, isBinary: false }
    }))
    mocks.openFile.mockReset().mockImplementation(async (path: string) => {
      mocks.openFiles.set(path, {})
    })
    mocks.updateCursorPosition.mockReset()
    mocks.addEditorTab.mockReset()
    mocks.toastError.mockReset()
    mocks.openFiles.clear()
  })

  it.each([
    ['See src/x.ts#L10 for the bug.', 'src/x.ts#L10', '/repo/src/x.ts', 10],
    ['See src/x.ts#L10-L20 for the bug.', 'src/x.ts#L10-L20', '/repo/src/x.ts', 10],
    ['Edit `src/x.ts#L10` next.', 'src/x.ts#L10', '/repo/src/x.ts', 10],
    ['Edit `main.rs` next.', 'main.rs', '/repo/main.rs', null],
    ['Open `out.png` to check.', 'out.png', '/repo/out.png', null],
    ['Edit `main.rs:12` next.', 'main.rs:12', '/repo/main.rs', 12]
  ])('%s opens %s at %s line %s', async (markdown, label, target, line) => {
    const container = renderChat(markdown)

    const button = await waitFor(() => {
      const found = container.querySelector<HTMLElement>('button[data-testid="termul-file-path"]')
      expect(found).not.toBeNull()
      return found as HTMLElement
    })
    expect(button).toHaveTextContent(label)
    fireEvent.click(button)

    await waitFor(() => expect(mocks.addEditorTab).toHaveBeenCalledWith(target))
    expect(mocks.getFileInfo).toHaveBeenCalledWith(target)
    expect(mocks.openFile).toHaveBeenCalledWith(target)
    if (line === null) {
      expect(mocks.updateCursorPosition).not.toHaveBeenCalled()
    } else {
      expect(mocks.updateCursorPosition).toHaveBeenCalledWith(target, line, 1)
    }
    expect(mocks.toastError).not.toHaveBeenCalled()
  })

  it('keeps an inline-code filename styled as inline code inside the button', async () => {
    const container = renderChat('Edit `main.rs` next.')

    await waitFor(() => {
      const code = container.querySelector(
        'button[data-testid="termul-file-path"] code[data-streamdown="inline-code"]'
      )
      expect(code).toHaveTextContent('main.rs')
    })
  })

  it.each([
    'Run main.rs now.',
    'Call `foo` first.',
    'Pick one, e.g. the first.',
    'Pick one, `e.g.` the first.',
    'Use `console.log` and `process.env` here.',
    'Bump `v1.2` and `user.id` first.'
  ])('leaves %s unlinked', async (markdown) => {
    const container = renderChat(markdown)

    await waitFor(() => expect(container.textContent).not.toBe(''))
    expect(container.querySelector('[data-testid="termul-file-path"]')).toBeNull()
  })
})
