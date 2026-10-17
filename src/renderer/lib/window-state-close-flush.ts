let closeWriter: (() => Promise<void>) | null = null

export function registerWindowStateCloseWriter(writer: () => Promise<void>): () => void {
  closeWriter = writer
  return () => {
    if (closeWriter === writer) closeWriter = null
  }
}

export function persistWindowStateBeforeClose(): Promise<void> {
  return closeWriter ? closeWriter() : Promise.resolve()
}
