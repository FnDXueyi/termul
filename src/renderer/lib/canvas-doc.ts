/**
 * Which files are canvas documents. One rule shared by the file explorer's
 * open branch and the "Open canvas" command.
 */

import type { DirectoryEntry } from '@shared/types/filesystem.types'

export const CANVAS_DOC_EXTENSION = '.figcraft'

/** What a project without a canvas document gets on its first open. */
export const NEW_CANVAS_DOC_NAME = 'design.figcraft'

/** `path` with forward slashes and without a Windows verbatim prefix:
 * `\\?\D:\p` is `D:/p` and `\\?\UNC\srv\share` is `//srv/share`. */
export function plainPath(path: string): string {
  return path
    .replace(/\\/g, '/')
    .replace(/^\/\/\?\/UNC\//i, '//')
    .replace(/^\/\/\?\//, '')
}

/** `plainPath`, folded to lower case for a drive-letter or UNC path, which
 * Windows compares without case. */
export function comparablePath(path: string): string {
  const plain = plainPath(path)
  return /^([A-Za-z]:|\/\/)/.test(plain) ? plain.toLowerCase() : plain
}

/** Whether two spellings name the same document: the command palette and
 * the file explorer hand the same file over with different separators. */
export function sameCanvasDoc(a: string, b: string): boolean {
  return comparablePath(a) === comparablePath(b)
}

export function isCanvasDocPath(path: string): boolean {
  return path.toLowerCase().endsWith(CANVAS_DOC_EXTENSION)
}

/** The project's alphabetically-first `.figcraft` file, or `undefined`. */
export function pickCanvasDoc(entries: DirectoryEntry[]): DirectoryEntry | undefined {
  return entries
    .filter((entry) => entry.type === 'file' && isCanvasDocPath(entry.name))
    .sort((a, b) => a.name.localeCompare(b.name))[0]
}
