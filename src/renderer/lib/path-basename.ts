/** Last path segment, splitting on both `/` and `\`. A path ending in a separator returns the input unchanged. */
export function basename(p: string): string {
  const parts = p.split(/[\\/]/)
  return parts[parts.length - 1] || p
}
