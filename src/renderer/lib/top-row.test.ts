import { afterEach, describe, expect, it, vi } from 'vitest'
import type { PaneNode } from '@/types/workspace.types'

const platformMock = vi.hoisted(() => ({ isMac: false, tauri: false }))

vi.mock('@/lib/platform', () => ({
  get isMac() {
    return platformMock.isMac
  }
}))
vi.mock('@/lib/tauri-runtime', () => ({ isTauriContext: () => platformMock.tauri }))

import {
  getRightmostTopEdgeLeafId,
  getTopEdgeLeafIds,
  getTopRowPlatform,
  paneTopRowRole,
  selectTopRowRoles
} from './top-row'

const leaf = (id: string): PaneNode => ({ type: 'leaf', id, tabs: [], activeTabId: null })
const split = (direction: 'horizontal' | 'vertical', ...children: PaneNode[]): PaneNode => ({
  type: 'split',
  id: `s-${children.map((c) => c.id).join('')}`,
  direction,
  children,
  sizes: children.map(() => 100 / children.length)
})
const H = (...c: PaneNode[]): PaneNode => split('horizontal', ...c)
const V = (...c: PaneNode[]): PaneNode => split('vertical', ...c)

const A = leaf('A')
const B = leaf('B')
const C = leaf('C')
const D = leaf('D')

describe('top-row selectors', () => {
  it.each([
    ['one pane', A, ['A'], 'A'],
    ['horizontal split', H(A, B), ['A', 'B'], 'B'],
    ['vertical split', V(A, B), ['A'], 'A'],
    ['H[A, V[B, C]]', H(A, V(B, C)), ['A', 'B'], 'B'],
    ['H[V[A, B], C]', H(V(A, B), C), ['A', 'C'], 'C'],
    ['V[H[A, B], H[C, D]]', V(H(A, B), H(C, D)), ['A', 'B'], 'B']
  ] as const)('%s', (_name, root, topEdge, rightmost) => {
    expect([...getTopEdgeLeafIds(root)].sort()).toEqual([...topEdge])
    expect(getRightmostTopEdgeLeafId(root)).toBe(rightmost)
  })

  it('treats a fullscreen pane as the whole tree', () => {
    const roles = selectTopRowRoles({
      root: V(A, H(B, C)),
      fullscreenPaneId: 'C',
      mode: 'panes'
    })
    expect([...roles.topEdgeLeafIds]).toEqual(['C'])
    expect(roles.rightmostLeafId).toBe('C')
    expect(roles.controlsOwner).toBe('pane')
  })

  it('ignores a fullscreen id that is not in the tree', () => {
    const roles = selectTopRowRoles({ root: H(A, B), fullscreenPaneId: 'Z', mode: 'panes' })
    expect(roles.rightmostLeafId).toBe('B')
  })

  it('gives the SSH header and the fallback bar the controls with no pane roles', () => {
    const ssh = selectTopRowRoles({ root: H(A, B), fullscreenPaneId: null, mode: 'ssh' })
    expect(ssh.controlsOwner).toBe('ssh-header')
    expect(ssh.topEdgeLeafIds.size).toBe(0)
    expect(ssh.rightmostLeafId).toBeNull()
    const fallback = selectTopRowRoles({ root: H(A, B), fullscreenPaneId: null, mode: 'fallback' })
    expect(fallback.controlsOwner).toBe('fallback-bar')
    expect(fallback.topEdgeLeafIds.size).toBe(0)
    expect(fallback.rightmostLeafId).toBeNull()
  })
})

describe('getTopRowPlatform', () => {
  afterEach(() => {
    platformMock.isMac = false
    platformMock.tauri = false
  })

  it('is all false on web', () => {
    expect(getTopRowPlatform()).toEqual({
      dragRegions: false,
      windowControls: false,
      trafficLights: false
    })
  })

  it('gives Windows and Linux desktop controls and no clearance', () => {
    platformMock.tauri = true
    expect(getTopRowPlatform()).toEqual({
      dragRegions: true,
      windowControls: true,
      trafficLights: false
    })
  })

  it('gives macOS desktop clearance and no controls', () => {
    platformMock.tauri = true
    platformMock.isMac = true
    expect(getTopRowPlatform()).toEqual({
      dragRegions: true,
      windowControls: false,
      trafficLights: true
    })
  })

  it('keeps a Mac browser on the web branch', () => {
    platformMock.isMac = true
    expect(getTopRowPlatform().trafficLights).toBe(false)
  })
})

describe('paneTopRowRole', () => {
  const desktop = { dragRegions: true, windowControls: true, trafficLights: false }
  const roles = selectTopRowRoles({ root: H(A, V(B, C)), fullscreenPaneId: null, mode: 'panes' })

  it('gives controls only to the rightmost top-edge pane', () => {
    expect(paneTopRowRole(roles, desktop, 'A')).toEqual({
      topEdge: true,
      dragRegion: true,
      windowControls: false
    })
    expect(paneTopRowRole(roles, desktop, 'B')).toEqual({
      topEdge: true,
      dragRegion: true,
      windowControls: true
    })
    expect(paneTopRowRole(roles, desktop, 'C')).toEqual({
      topEdge: false,
      dragRegion: false,
      windowControls: false
    })
  })

  it('drops drag and controls on web and controls on macOS', () => {
    const web = { dragRegions: false, windowControls: false, trafficLights: false }
    expect(paneTopRowRole(roles, web, 'B')).toEqual({
      topEdge: true,
      dragRegion: false,
      windowControls: false
    })
    const mac = { dragRegions: true, windowControls: false, trafficLights: true }
    expect(paneTopRowRole(roles, mac, 'B').windowControls).toBe(false)
  })

  it('gives no pane the controls when the SSH header owns them', () => {
    const ssh = selectTopRowRoles({ root: A, fullscreenPaneId: null, mode: 'ssh' })
    expect(paneTopRowRole(ssh, desktop, 'A').windowControls).toBe(false)
  })
})
