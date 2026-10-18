import type { Locator, Page } from 'playwright'

export interface Shot {
  /** Saved as public/screenshots/<id>.png and embedded as /screenshots/<id>.png. */
  id: string
  /** Defaults to a 1280x800 desktop window. */
  viewport?: { width: number; height: number }
  /** Return a locator to crop the image to that element. */
  run: (page: Page) => Promise<Locator | undefined | void>
}
