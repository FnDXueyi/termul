import { readdirSync, statSync } from 'node:fs'
import { join, relative, sep } from 'node:path'

/**
 * Source scanning for raw `<input>`, `<textarea>` and `<select>` tags in the
 * renderer, shared by the text-field guard tests in `components/ui/`.
 */

export const NON_TEXT_TYPES: ReadonlySet<string> = new Set([
  'checkbox',
  'radio',
  'range',
  'file',
  'hidden',
  'button',
  'submit',
  'reset',
  'image',
  'color'
])

/** A tag at the start of a line, or right after `&&`, `?`, `:`, `=>` or `(` in a JSX expression. */
export const TAG_START = /(?:^[ \t]*|(?:&&|\?|:|=>|\()[ \t]*)<(?:input|textarea|select)(?![\w-])/gm

const LITERAL_TYPE = /(?<![\w-])type=(?:"([^"]*)"|'([^']*)'|\{\s*["']([^"']*)["']\s*\})/

/** Reads one opening tag from its `<` to its closing `>`, skipping `{}` bodies, quotes and comments. */
export function readTag(source: string, start: number): string {
  let depth = 0
  let quote: string | null = null
  for (let i = start; i < source.length; i++) {
    const char = source[i]
    if (quote) {
      if (char === '\\') i++
      else if (char === quote) quote = null
      continue
    }
    if (char === '/' && source[i + 1] === '/') {
      const end = source.indexOf('\n', i)
      if (end === -1) break
      i = end
      continue
    }
    if (char === '/' && source[i + 1] === '*') {
      const end = source.indexOf('*/', i + 2)
      if (end === -1) break
      i = end + 1
      continue
    }
    if (char === '"' || char === "'" || char === '`') quote = char
    else if (char === '{') depth++
    else if (char === '}') depth--
    else if (char === '>' && depth === 0 && source[i - 1] !== '=') return source.slice(start, i + 1)
  }
  return source.slice(start)
}

/** True when the tag has a literal `type` that takes no text, such as a checkbox. */
export function isNonTextTag(tag: string): boolean {
  const type = LITERAL_TYPE.exec(tag)
  return type !== null && NON_TEXT_TYPES.has(type[1] ?? type[2] ?? type[3])
}

export interface RawFieldTag {
  line: number
  tag: string
}

/** Every raw text-entry tag in `source`, with its 1-based line. Non-text input types are skipped. */
export function findTextFieldTags(source: string): RawFieldTag[] {
  const tags: RawFieldTag[] = []
  for (const match of source.matchAll(TAG_START)) {
    const start = match.index + match[0].indexOf('<')
    const tag = readTag(source, start)
    if (isNonTextTag(tag)) continue
    tags.push({ line: source.slice(0, start).split('\n').length, tag })
  }
  return tags
}

/** Non-test `.tsx` files under `dir`, sorted. */
export function listSourceFiles(dir: string): string[] {
  const files: string[] = []
  for (const name of readdirSync(dir).sort()) {
    if (name === 'node_modules') continue
    const path = join(dir, name)
    if (statSync(path).isDirectory()) files.push(...listSourceFiles(path))
    else if (name.endsWith('.tsx') && !name.endsWith('.test.tsx')) files.push(path)
  }
  return files
}

/** `path` relative to `root`, with forward slashes. */
export function toRootPath(root: string, path: string): string {
  return relative(root, path).split(sep).join('/')
}
