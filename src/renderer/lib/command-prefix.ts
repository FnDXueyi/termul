/**
 * A slash command is a prefix of the composer draft, not a word in the sentence.
 *
 * The wire text is `/name` plus the remaining sentence. The pill is placed at
 * the start when the user picks it, and a draft that still has the token in
 * the middle is rewritten to that same shape so the composer, the bubble, and
 * the model agree before Enter.
 */
import {
  CMD_TOKEN_END,
  CMD_TOKEN_START,
  commandToken,
  extractCommandName,
  type InsertCommandTokenResult
} from '@/lib/skill-tokens'

interface CommandSpan {
  start: number
  /** Index of the closing sentinel. */
  end: number
  name: string
}

function findFirstCommand(value: string): CommandSpan | null {
  const start = value.indexOf(CMD_TOKEN_START)
  if (start < 0) return null
  const end = value.indexOf(CMD_TOKEN_END, start + 1)
  if (end < 0) return null
  const name = value.slice(start + 1, end)
  if (name.length === 0) return null
  return { start, end, name }
}

/**
 * Join `before` and `after` where a command token was removed. A run of spaces
 * at the join collapses to one space. Two words with no space gain one space.
 * A line break at the join is left as it is.
 */
function restAfterRemoval(value: string, start: number, removeEnd: number): string {
  const before = value.slice(0, start)
  const after = value.slice(removeEnd)
  if (before.length === 0) return after.replace(/^[ \t]+/, '')
  if (after.length === 0) return before.replace(/^[ \t]+/, '')
  const rest = before + after
  const join = before.length
  let left = join
  while (left > 0 && rest[left - 1] === ' ') left -= 1
  let right = join
  while (right < rest.length && rest[right] === ' ') right += 1
  let joined = rest
  if (left < join || right > join) {
    joined = `${rest.slice(0, left)} ${rest.slice(right)}`
  } else if (rest[join - 1] !== '\n' && rest[join] !== '\n') {
    joined = `${before} ${after}`
  }
  return joined.replace(/^[ \t]+/, '')
}

/**
 * Move the first command token to the start of `value`. A value that is
 * already prefixed is returned unchanged. Text with no command token is
 * returned unchanged.
 */
export function normalizeCommandToPrefix(value: string): string {
  const found = findFirstCommand(value)
  if (!found) return value
  let removeEnd = found.end + 1
  if (value[removeEnd] === ' ') removeEnd += 1
  const rest = restAfterRemoval(value, found.start, removeEnd)
  const token = commandToken(found.name)
  if (rest.length === 0) {
    if (found.start === 0) return value
    return `${token} `
  }
  const next = `${token} ${rest}`
  return next === value ? value : next
}

function isSpaceBeforeToken(value: string, index: number): boolean {
  if (index <= 0) return true
  const prev = value[index - 1]
  return prev === ' ' || prev === '\t' || prev === '\n' || prev === '\r'
}

/**
 * Map a caret in `value` onto `normalized` after {@link normalizeCommandToPrefix}.
 *
 * Text before the token follows that text, which now sits after the prefix.
 * A caret on the pill itself (the token start, with whitespace before it)
 * follows the pill to offset 0. A caret glued to a character that was just
 * typed against the pill stays after that character. A caret inside the token
 * lands just after the prefix.
 */
export function mapCaretAfterCommandPrefix(
  value: string,
  normalized: string,
  caret: number
): number {
  const clamp = (n: number): number => Math.max(0, Math.min(n, normalized.length))
  if (normalized === value) return clamp(caret)
  const found = findFirstCommand(value)
  if (!found) return clamp(caret)
  let removeEnd = found.end + 1
  if (value[removeEnd] === ' ') removeEnd += 1
  const prefixLen = commandToken(found.name).length + 1
  const safe = Math.max(0, Math.min(caret, value.length))
  const lead = /^[ \t]*/.exec(value.slice(0, found.start))?.[0].length ?? 0
  if (safe < found.start) {
    return clamp(prefixLen + Math.max(0, safe - lead))
  }
  // The display caret on a pill is the token start. When a space separates
  // that pill from the sentence, follow the pill to the front. A character
  // typed against the pill has no space, so the caret stays after that text.
  if (safe === found.start && isSpaceBeforeToken(value, found.start)) {
    return 0
  }
  if (safe === found.start) {
    return clamp(prefixLen + Math.max(0, safe - lead))
  }
  if (safe >= removeEnd) {
    const after = value.slice(removeEnd)
    const rest = restAfterRemoval(value, found.start, removeEnd)
    const beforeInRest = rest.length - after.length
    return clamp(prefixLen + beforeInRest + (safe - removeEnd))
  }
  return clamp(prefixLen)
}

/**
 * Remove the `/filter` at the caret and place one command token at the start.
 * The caret lands at the end of the remaining sentence. A draft that already
 * has a command token is left unchanged.
 */
export function placeCommandTokenAtStart(
  value: string,
  caret: number,
  name: string,
  deleteBefore = 0
): InsertCommandTokenResult {
  if (extractCommandName(value) !== null) {
    return { inserted: false, reason: 'existing_command' }
  }
  const start = Math.max(0, Math.min(caret - deleteBefore, value.length))
  const end = Math.max(start, Math.min(caret, value.length))
  const without = `${value.slice(0, start)}${value.slice(end)}`
  const rest = restAfterRemoval(without, start, start)
  const token = commandToken(name)
  const next = rest.length === 0 ? `${token} ` : `${token} ${rest}`
  return { inserted: true, value: next, caret: next.length }
}
