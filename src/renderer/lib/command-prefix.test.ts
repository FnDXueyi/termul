import { describe, expect, it } from 'vitest'
import {
  mapCaretAfterCommandPrefix,
  normalizeCommandToPrefix,
  placeCommandTokenAtStart
} from '@/lib/command-prefix'
import { commandToken } from '@/lib/skill-tokens'

const token = commandToken('compact')

describe('normalizeCommandToPrefix', () => {
  it('returns text with no command unchanged', () => {
    expect(normalizeCommandToPrefix('buatkan design baru')).toBe('buatkan design baru')
  })

  it('keeps a command that is already the prefix', () => {
    const value = `${token} buatkan design baru`
    expect(normalizeCommandToPrefix(value)).toBe(value)
  })

  it('moves a mid-sentence command to the front', () => {
    expect(normalizeCommandToPrefix(`buatkan ${token} design baru`)).toBe(
      `${token} buatkan design baru`
    )
  })

  it('keeps a typed character that was inserted before the pill', () => {
    expect(normalizeCommandToPrefix(`Z${token} hello`)).toBe(`${token} Z hello`)
  })

  it('collapses the spaces left around the old position', () => {
    expect(normalizeCommandToPrefix(`hello  ${token}  world`)).toBe(`${token} hello world`)
  })
})

describe('mapCaretAfterCommandPrefix', () => {
  it('keeps the caret when the value does not change', () => {
    const value = `${token} hello`
    expect(mapCaretAfterCommandPrefix(value, value, 3)).toBe(3)
  })

  it('places the caret after a character typed in front of the pill', () => {
    const value = `Z${token} hello`
    const normalized = normalizeCommandToPrefix(value)
    expect(mapCaretAfterCommandPrefix(value, normalized, 1)).toBe(token.length + 2)
  })

  it('follows a caret on a mid-sentence pill to the front', () => {
    const value = `hello ${token} world`
    const normalized = normalizeCommandToPrefix(value)
    const pill = value.indexOf(token)
    expect(pill).toBeGreaterThan(1)
    expect(mapCaretAfterCommandPrefix(value, normalized, pill)).toBe(0)
  })

  it('does not drop a caret on the pill into the sentence when spaces collapse', () => {
    const value = `hello  ${token}  world`
    const normalized = normalizeCommandToPrefix(value)
    expect(mapCaretAfterCommandPrefix(value, normalized, value.indexOf(token))).toBe(0)
  })

  it('keeps a caret inside the text before the pill with that text', () => {
    const value = `hello ${token} world`
    const normalized = normalizeCommandToPrefix(value)
    expect(mapCaretAfterCommandPrefix(value, normalized, 1)).toBe(token.length + 2)
  })

  it('keeps a caret at the end of the sentence at the end', () => {
    const value = `hello ${token} world`
    const normalized = normalizeCommandToPrefix(value)
    expect(mapCaretAfterCommandPrefix(value, normalized, value.length)).toBe(normalized.length)
  })
})

describe('placeCommandTokenAtStart', () => {
  it('removes a trailing slash filter and prefixes the command', () => {
    const value = 'buatkan design baru /compact'
    const result = placeCommandTokenAtStart(value, value.length, 'compact', '/compact'.length)
    expect(result.inserted).toBe(true)
    if (!result.inserted) return
    expect(result.value).toBe(`${token} buatkan design baru `)
    expect(result.caret).toBe(result.value.length)
  })

  it('removes a mid-sentence slash filter and prefixes the command', () => {
    const value = 'hello /compact world'
    const slash = value.indexOf('/compact')
    const result = placeCommandTokenAtStart(
      value,
      slash + '/compact'.length,
      'compact',
      '/compact'.length
    )
    expect(result.inserted).toBe(true)
    if (!result.inserted) return
    expect(result.value).toBe(`${token} hello world`)
  })

  it('rejects a second command', () => {
    const result = placeCommandTokenAtStart(`${token} hello /clear`, 20, 'clear', 6)
    expect(result).toEqual({ inserted: false, reason: 'existing_command' })
  })
})
