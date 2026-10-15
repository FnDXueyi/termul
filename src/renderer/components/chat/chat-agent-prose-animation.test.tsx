import { act, render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { AgentProse } from './chat-agent-prose'

interface WordTiming {
  text: string
  duration: number
  delay: number
}

const FIRST = 'Alpha beta gamma delta epsilon zeta eta theta'
const SECOND = `${FIRST} iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon`
const BACKLOG_CAP_MS = 320

// The streaming text commits at most every 100 ms; wait past that window.
const settle = () => act(async () => new Promise<void>((resolve) => setTimeout(resolve, 250)))

function wordTimings(container: HTMLElement): WordTiming[] {
  return [...container.querySelectorAll<HTMLElement>('[data-sd-animate]')].map((el) => ({
    text: (el.textContent ?? '').trim(),
    duration: Number.parseFloat(el.style.getPropertyValue('--sd-duration')),
    delay: Number.parseFloat(el.style.getPropertyValue('--sd-delay') || '0')
  }))
}

function renderProse(text: string, streaming: boolean) {
  const ui = (value: string, isStreaming: boolean) => (
    <TooltipProvider>
      <AgentProse text={value} streaming={isStreaming} reduced={false} />
    </TooltipProvider>
  )
  const view = render(ui(text, streaming))
  return {
    ...view,
    update: (value: string, isStreaming: boolean) => view.rerender(ui(value, isStreaming))
  }
}

describe('AgentProse streaming word animation', () => {
  it('staggers each new word after the previous one, in reading order', async () => {
    const view = renderProse(FIRST, true)
    await settle()
    view.update(SECOND, true)
    await settle()

    const words = wordTimings(view.container)
    const firstWords = FIRST.split(' ')
    const fresh = words.filter((w) => !firstWords.includes(w.text))

    expect(fresh.length).toBe(SECOND.split(' ').length - firstWords.length)
    for (const word of fresh) expect(word.duration).toBeGreaterThan(0)

    // Stagger is kept (words do not all start together) and never runs backwards.
    const delays = fresh.map((w) => w.delay)
    for (let i = 1; i < delays.length; i++) expect(delays[i]).toBeGreaterThan(delays[i - 1])
    expect(delays.at(-1)! - delays[0]).toBeGreaterThan(0)

    // The reveal of one commit stays within the shared backlog window.
    expect(delays.at(-1)!).toBeLessThanOrEqual(BACKLOG_CAP_MS)
  })

  it('queues a follow-up burst behind words that are still revealing', async () => {
    const burst = Array.from({ length: 40 }, (_, i) => `w${i}`).join(' ')
    const view = renderProse(burst, true)
    // Just past the 100 ms commit window, while most of the burst is still fading in.
    await act(async () => new Promise<void>((resolve) => setTimeout(resolve, 130)))
    view.update(`${burst} next after burst`, true)
    await act(async () => new Promise<void>((resolve) => setTimeout(resolve, 60)))

    const words = wordTimings(view.container)
    const next = words.find((w) => w.text === 'next')
    expect(next?.duration).toBeGreaterThan(0)
    // A fresh word must not restart at delay 0 and overtake the earlier burst.
    expect(next?.delay).toBeGreaterThan(0)
  })

  it('keeps words from earlier commits steady instead of replaying them', async () => {
    const view = renderProse(FIRST, true)
    await settle()
    view.update(SECOND, true)
    await settle()

    const firstWords = FIRST.split(' ')
    const earlier = wordTimings(view.container).filter((w) => firstWords.includes(w.text))

    expect(earlier.length).toBe(firstWords.length)
    for (const word of earlier) {
      expect(word.duration).toBe(0)
      expect(word.delay).toBe(0)
    }
  })

  it('drops the animation wrappers once the turn settles', async () => {
    const view = renderProse(SECOND, true)
    await settle()
    expect(view.container.querySelectorAll('[data-sd-animate]').length).toBeGreaterThan(0)

    view.update(SECOND, false)
    await settle()
    expect(view.container.querySelectorAll('[data-sd-animate]').length).toBe(0)
    expect(view.container.textContent).toContain('upsilon')
  })
})
