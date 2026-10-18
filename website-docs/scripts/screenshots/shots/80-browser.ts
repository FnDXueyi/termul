import { writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { Page } from 'playwright'
import { DEMO_WORKSPACE_ROOT } from '../demo-server'
import type { Shot } from '../shot'
import { openApp } from '../ui'

// Browser tabs, page annotations and agent browser control are desktop-only,
// so this section only screenshots the canvas entry points. The demo server
// has no OpenPencil editor server, which is also the state of a fresh install.

async function writeDesign(): Promise<void> {
  await writeFile(
    join(DEMO_WORKSPACE_ROOT, 'acme-shop', 'design.op'),
    `${JSON.stringify({ version: '1.0.0', name: 'Acme Shop', children: [] }, null, 2)}\n`
  )
}

async function openNewTabMenu(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'New tab', exact: true }).first().click()
  await page.getByRole('menuitem', { name: 'Open canvas' }).waitFor()
}

function toast(page: Page, text: string) {
  return page.locator('[data-sonner-toast]').filter({ hasText: text }).first()
}

export const shots: Shot[] = [
  {
    id: 'browser-canvas-new-tab-menu',
    run: async (page) => {
      await openApp(page)
      await openNewTabMenu(page)
      return page.getByRole('menu')
    }
  },
  {
    id: 'browser-canvas-palette',
    run: async (page) => {
      await openApp(page)
      await page.getByRole('button', { name: 'Search', exact: true }).first().click()
      await page.keyboard.type('canvas')
      await page.getByText('Open Canvas', { exact: true }).waitFor()
    }
  },
  {
    id: 'browser-canvas-no-document',
    run: async (page) => {
      await openApp(page, 'blog')
      await openNewTabMenu(page)
      await page.getByRole('menuitem', { name: 'Open canvas' }).click()
      const message = toast(page, 'No .op document found')
      await message.waitFor()
      return message
    }
  },
  {
    id: 'browser-canvas-no-editor',
    run: async (page) => {
      await writeDesign()
      await openApp(page)
      await openNewTabMenu(page)
      await page.getByRole('menuitem', { name: 'Open canvas' }).click()
      const message = toast(page, 'Could not open the canvas')
      await message.waitFor({ timeout: 20_000 })
      return message
    }
  }
]
