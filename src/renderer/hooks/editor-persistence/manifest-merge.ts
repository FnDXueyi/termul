/**
 * Merge-back of non-portable tabs after a host-manifest restore (#903).
 *
 * The manifest owns the portable topology (terminal/editor tabs, active tab,
 * active pane) but cannot carry renderer-only tab bodies (browser, git,
 * git-history, later diff). Those live only in `editor-state/<projectId>
 * .paneLayout`, so a manifest restore alone would drop them. This module puts
 * them back over the manifest-built tree without changing what the manifest
 * decided: existing tabs, the manifest's active tab per pane and the active
 * pane stay as they are. Agent chats are excluded — `reattachOpenAgentChats`
 * owns them.
 *
 * Kind selection is driven by `WORKSPACE_TAB_KIND_TRAITS`
 * (`persisted && !manifestPortable`), so a new non-portable kind is picked up
 * without an edit here.
 */

import { logFrontendError } from '@/lib/log-api'
import { isWorkspaceTabKind, WORKSPACE_TAB_KIND_TRAITS } from '@/lib/workspace-tab-kinds'
import type { WorkspaceTab } from '@/stores/workspace-store'
import { browserTabId, getAllLeafPanes } from '@/stores/workspace-store'
import type { LeafNode, PaneNode } from '@/types/workspace.types'
import { deserializePaneTree } from './pane-tree'
import type { PersistedPaneNodeInput, PersistedTabRef } from './types'

const SOURCE = 'useEditorPersistence.mergeNonPortableTabs'

interface PersistedLeafSlice {
  id: string
  refs: PersistedTabRef[]
  activeTabId: string | null
}

function isMergeBackKind(kind: unknown): boolean {
  if (!isWorkspaceTabKind(kind) || kind === 'agent-chat') return false
  const traits = WORKSPACE_TAB_KIND_TRAITS[kind]
  return traits.persisted && !traits.manifestPortable
}

function collectLeafSlices(node: PersistedPaneNodeInput | undefined): PersistedLeafSlice[] {
  if (!node || typeof node !== 'object') return []
  if (node.type === 'split') {
    return Array.isArray(node.children) ? node.children.flatMap(collectLeafSlices) : []
  }
  // A legacy `editorFilePaths` leaf carries editor tabs only: nothing to merge.
  if (node.type !== 'leaf' || !('tabs' in node) || !Array.isArray(node.tabs)) return []
  const refs = node.tabs.filter(
    (ref): ref is PersistedTabRef =>
      ref != null && typeof ref === 'object' && isMergeBackKind((ref as { type?: unknown }).type)
  )
  return refs.length > 0 ? [{ id: node.id, refs, activeTabId: node.activeTabId ?? null }] : []
}

/** The tab id a ref will get, when it is knowable without building the tab. */
function knownRefTabId(ref: PersistedTabRef): string | null {
  switch (ref.type) {
    case 'browser':
      return typeof ref.browserTabId === 'string' ? browserTabId(ref.browserTabId) : null
    case 'git':
    case 'git-history':
      return typeof ref.id === 'string' ? ref.id : null
    default:
      return null
  }
}

function collectTabIds(root: PaneNode): Set<string> {
  return new Set(getAllLeafPanes(root).flatMap((leaf) => leaf.tabs.map((tab) => tab.id)))
}

function appendToLeaves(
  node: PaneNode,
  additions: ReadonlyMap<string, { tabs: WorkspaceTab[]; persistedActiveTabId: string | null }>
): PaneNode {
  if (node.type === 'split') {
    return { ...node, children: node.children.map((child) => appendToLeaves(child, additions)) }
  }
  const added = additions.get(node.id)
  if (!added) return node
  const leaf: LeafNode = { ...node, tabs: [...node.tabs, ...added.tabs] }
  if (
    leaf.activeTabId === null &&
    added.tabs.some((tab) => tab.id === added.persistedActiveTabId)
  ) {
    leaf.activeTabId = added.persistedActiveTabId
  }
  return leaf
}

/**
 * Append the layout's non-portable tabs to a manifest-built tree: to the leaf
 * with the same id, else the active pane (else the first leaf). Pure over the
 * tree — it returns the merged root (the SAME `root` when nothing was added)
 * and the number of tabs added; the caller decides where the result goes.
 */
export function mergeNonPortableTabs(
  root: PaneNode,
  activePaneId: string | null,
  layout: PersistedPaneNodeInput | undefined,
  projectId: string
): { root: PaneNode; added: number } {
  const slices = collectLeafSlices(layout)
  if (slices.length === 0) return { root, added: 0 }

  const leaves = getAllLeafPanes(root)
  const fallbackLeaf = leaves.find((leaf) => leaf.id === activePaneId) ?? leaves[0]
  if (!fallbackLeaf) return { root, added: 0 }

  const seen = collectTabIds(root)
  const additions = new Map<string, { tabs: WorkspaceTab[]; persistedActiveTabId: string | null }>()
  let added = 0
  for (const slice of slices) {
    const refs = slice.refs.filter((ref) => {
      const id = knownRefTabId(ref)
      return id === null || !seen.has(id)
    })
    if (refs.length === 0) continue
    const built = deserializePaneTree(
      { type: 'leaf', id: slice.id, tabs: refs, activeTabId: slice.activeTabId },
      projectId
    )
    if (built.type !== 'leaf') continue
    const tabs = built.tabs.filter((tab) => !seen.has(tab.id))
    if (tabs.length === 0) continue
    for (const tab of tabs) seen.add(tab.id)
    const targetId = leaves.some((leaf) => leaf.id === slice.id) ? slice.id : fallbackLeaf.id
    const entry = additions.get(targetId) ?? { tabs: [], persistedActiveTabId: null }
    entry.tabs.push(...tabs)
    entry.persistedActiveTabId ??= slice.activeTabId
    additions.set(targetId, entry)
    added += tabs.length
  }
  if (added === 0) return { root, added: 0 }

  void logFrontendError({
    level: 'info',
    source: SOURCE,
    message: `Merged ${added} non-portable tab(s) from editor-state into the manifest restore for project ${projectId}`
  })
  return { root: appendToLeaves(root, additions), added }
}
