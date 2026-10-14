import type React from 'react'
import { cn } from '@/lib/utils'

// Avatar fills cycle through the project palette tokens (see index.css).
const AVATAR_COLORS = [
  'oklch(var(--project-orange))',
  'oklch(var(--project-green))',
  'oklch(var(--project-purple))',
  'oklch(var(--project-blue))',
  'oklch(var(--project-cyan))',
  'oklch(var(--project-pink))'
]

function avatarColor(name: string): string {
  let hash = 0
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) >>> 0
  return AVATAR_COLORS[hash % AVATAR_COLORS.length]
}

/** Initial on a palette dot, stable per author name. Decorative: the name is
 * always shown next to it. */
export function AuthorAvatar({
  name,
  size = 16,
  className
}: {
  name: string
  size?: number
  className?: string
}): React.JSX.Element {
  const initial = name.trim().charAt(0).toUpperCase() || '?'
  return (
    <span
      aria-hidden="true"
      className={cn(
        'inline-flex shrink-0 items-center justify-center rounded-full font-semibold leading-none text-background',
        className
      )}
      style={{
        width: size,
        height: size,
        fontSize: Math.round(size * 0.56),
        backgroundColor: avatarColor(name)
      }}
    >
      {initial}
    </span>
  )
}
