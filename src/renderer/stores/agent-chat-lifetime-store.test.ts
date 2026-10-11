import { beforeEach, describe, expect, it } from 'vitest'
import { retainedAgentChatSessionIds, useAgentChatLifetimeStore } from './agent-chat-lifetime-store'

describe('agent chat lifetime', () => {
  beforeEach(() => {
    useAgentChatLifetimeStore.setState({
      retainedByProject: {},
      activeSessionByProject: {},
      focusSessionByProject: {},
      closingSessionIds: {}
    })
  })

  it('keeps an Agent chat when its Project leaves the screen, and drops it on close', () => {
    useAgentChatLifetimeStore.getState().retainProjectChats('project-a', ['s1', 's2'])
    useAgentChatLifetimeStore.getState().retainProjectChats('project-a', ['s2'])
    expect(useAgentChatLifetimeStore.getState().retainedByProject['project-a']).toEqual([
      's1',
      's2'
    ])
    useAgentChatLifetimeStore.getState().releaseChat('s1')
    expect(
      retainedAgentChatSessionIds(useAgentChatLifetimeStore.getState().retainedByProject)
    ).toEqual(new Set(['s2']))
  })

  it('opens the chat you left, and a needs-you click wins once', () => {
    const store = useAgentChatLifetimeStore.getState()
    store.rememberActiveChat('project-a', 's1')
    store.requestFocus('project-a', 's2')
    expect(useAgentChatLifetimeStore.getState().takeFocus('project-a')).toBe('s2')
    expect(useAgentChatLifetimeStore.getState().takeFocus('project-a')).toBe('s1')
  })

  it('marks Closing and clears it when the chat is released', () => {
    useAgentChatLifetimeStore.getState().markClosing('s1')
    expect(useAgentChatLifetimeStore.getState().closingSessionIds.s1).toBe(true)
    useAgentChatLifetimeStore.getState().releaseChat('s1')
    expect(useAgentChatLifetimeStore.getState().closingSessionIds.s1).toBeUndefined()
  })

  it('forgets a deleted Project: retained, remembered and one-shot focus entries', () => {
    const store = useAgentChatLifetimeStore.getState()
    store.retainProjectChats('project-a', ['s1'])
    store.retainProjectChats('project-b', ['s2'])
    store.rememberActiveChat('project-a', 's1')
    store.rememberActiveChat('project-b', 's2')
    store.requestFocus('project-a', 's1')
    store.markClosing('s1')

    useAgentChatLifetimeStore.getState().forgetProject('project-a')

    const next = useAgentChatLifetimeStore.getState()
    expect(next.retainedByProject).toEqual({ 'project-b': ['s2'] })
    expect(next.activeSessionByProject).toEqual({ 'project-b': 's2' })
    expect(next.focusSessionByProject).toEqual({})
    // Closing is keyed by session and stays with the close flow.
    expect(next.closingSessionIds).toEqual({ s1: true })
  })

  it('forgetProject is a no-op for an unknown or empty Project id', () => {
    useAgentChatLifetimeStore.getState().retainProjectChats('project-a', ['s1'])
    const before = useAgentChatLifetimeStore.getState().retainedByProject
    useAgentChatLifetimeStore.getState().forgetProject('missing')
    useAgentChatLifetimeStore.getState().forgetProject('')
    expect(useAgentChatLifetimeStore.getState().retainedByProject).toBe(before)
  })
})
