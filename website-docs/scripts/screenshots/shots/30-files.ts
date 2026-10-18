import { writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { Locator, Page } from 'playwright'
import { DEMO_WORKSPACE_ROOT } from '../demo-server'
import type { Shot } from '../shot'
import { openApp } from '../ui'

const SHOP = join(DEMO_WORKSPACE_ROOT, 'acme-shop')

async function openExplorer(page: Page): Promise<Locator> {
  await page.getByRole('tab', { name: 'Explorer', exact: true }).click()
  const panel = page.locator('#file-explorer-panel')
  await panel.locator('[data-path]').first().waitFor()
  return panel
}

function treeRow(panel: Locator, relPath: string): Locator {
  return panel.locator(`[data-path="${join(SHOP, relPath)}"]`)
}

async function expandFolder(panel: Locator, relPath: string): Promise<void> {
  await treeRow(panel, relPath).click()
  await panel
    .locator(`[data-path^="${join(SHOP, relPath)}/"]`)
    .first()
    .waitFor()
}

/** Opens `relPath` from the tree and waits for its editor tab. */
async function openFile(page: Page, panel: Locator, relPath: string): Promise<void> {
  const parts = relPath.split('/')
  for (let i = 1; i < parts.length; i++) {
    const folder = parts.slice(0, i).join('/')
    if (!(await treeRow(panel, `${folder}/${parts[i]}`).isVisible())) {
      await expandFolder(panel, folder)
    }
  }
  await treeRow(panel, relPath).click()
  await page
    .getByRole('tab', { name: parts[parts.length - 1] })
    .first()
    .waitFor()
  await page.mouse.move(1000, 700)
  await page.waitForTimeout(1_000)
}

async function editCart(page: Page): Promise<void> {
  await page.locator('.cm-content').first().click()
  await page.keyboard.press('Control+End')
  await page.keyboard.type('\nexport const FREE_SHIPPING_OVER = 50\n')
}

const shopWithSrc = async (page: Page): Promise<Locator> => {
  await openApp(page)
  const panel = await openExplorer(page)
  await expandFolder(panel, 'src')
  return panel
}

export const shots: Shot[] = [
  {
    id: 'files-explorer',
    run: async (page) => {
      const panel = await shopWithSrc(page)
      await openFile(page, panel, 'src/cart.ts')
    }
  },
  {
    id: 'files-context-menu',
    run: async (page) => {
      const panel = await shopWithSrc(page)
      await treeRow(panel, 'src').click({ button: 'right' })
      const menu = page.getByRole('menu')
      await menu.waitFor()
      return menu
    }
  },
  {
    id: 'files-new-file',
    run: async (page) => {
      const panel = await shopWithSrc(page)
      await treeRow(panel, 'src').click({ button: 'right' })
      await page.getByRole('menuitem', { name: 'New File' }).click()
      await page.keyboard.type('shipping.ts')
      await page.mouse.move(1000, 700)
      await page.waitForTimeout(500)
      return panel
    }
  },
  {
    id: 'files-delete-dialog',
    run: async (page) => {
      const panel = await shopWithSrc(page)
      await treeRow(panel, 'src/main.ts').click({ button: 'right' })
      await page.getByRole('menuitem', { name: /^Delete/ }).click()
      const dialog = page.getByRole('alertdialog')
      await dialog.waitFor()
      await page.waitForTimeout(400)
      return dialog
    }
  },
  {
    id: 'files-drag-split',
    run: async (page) => {
      const panel = await shopWithSrc(page)
      await openFile(page, panel, 'src/cart.ts')
      const editor = page.locator('.cm-editor').first()
      const box = await editor.boundingBox()
      if (!box) throw new Error('editor not visible')
      await treeRow(panel, 'src/checkout.ts').dragTo(editor, {
        targetPosition: { x: box.width - 30, y: box.height / 2 }
      })
      await page.getByRole('tab', { name: 'checkout.ts' }).first().waitFor()
      await page.mouse.move(1000, 700)
      await page.waitForTimeout(1_000)
    }
  },
  {
    id: 'files-editor-unsaved',
    run: async (page) => {
      const panel = await shopWithSrc(page)
      await openFile(page, panel, 'src/cart.ts')
      await editCart(page)
    }
  },
  {
    id: 'files-markdown-preview',
    run: async (page) => {
      await openApp(page)
      const panel = await openExplorer(page)
      await openFile(page, panel, 'docs/roadmap.md')
      await page.getByRole('region', { name: 'Properties' }).waitFor()
    }
  },
  {
    id: 'files-markdown-source',
    run: async (page) => {
      await openApp(page)
      const panel = await openExplorer(page)
      await openFile(page, panel, 'README.md')
      await page.getByRole('radio', { name: 'Source' }).click()
      await page.locator('.cm-content').first().waitFor()
      await page.mouse.move(1000, 700)
      await page.waitForTimeout(800)
    }
  },
  {
    id: 'files-outline-depth',
    run: async (page) => {
      await openApp(page)
      const panel = await openExplorer(page)
      await openFile(page, panel, 'README.md')
      await page.getByRole('button', { name: /^Heading depth/ }).click()
      const menu = page.getByRole('menu')
      await menu.waitFor()
      await page.waitForTimeout(400)
      return menu
    }
  },
  {
    id: 'files-mermaid',
    run: async (page) => {
      await openApp(page)
      const panel = await openExplorer(page)
      await openFile(page, panel, 'README.md')
      await page.getByRole('button', { name: 'Zoom in' }).waitFor()
      await page.waitForTimeout(500)
      return page.locator('.markdown-editor').first()
    }
  },
  {
    id: 'files-changed-externally',
    run: async (page) => {
      const panel = await shopWithSrc(page)
      await openFile(page, panel, 'src/cart.ts')
      await editCart(page)
      await page.waitForTimeout(2_500)
      await writeFile(
        join(SHOP, 'src/cart.ts'),
        '// Edited in another program\nexport const VERSION = 2\n'
      )
      const toast = page
        .locator('[data-sonner-toast]')
        .filter({ hasText: 'File changed externally' })
      await toast.waitFor()
      await page.waitForTimeout(500)
      return toast
    }
  },
  {
    id: 'files-draft-conflict',
    run: async (page) => {
      const panel = await shopWithSrc(page)
      await openFile(page, panel, 'src/checkout.ts')
      await editCart(page)
      await page.waitForTimeout(3_000)
      await writeFile(
        join(SHOP, 'src/checkout.ts'),
        '// Edited in another program\nexport const RETRIES = 3\n'
      )
      await page.waitForTimeout(1_500)
      await page.reload()
      await page.getByRole('alert').filter({ hasText: 'unsaved draft' }).waitFor()
      await page.waitForTimeout(800)
    }
  },
  {
    id: 'files-autosave-setting',
    run: async (page) => {
      await openApp(page)
      await page.keyboard.press('Alt+,')
      const label = page.getByText('Editor Auto Save', { exact: true })
      await label.waitFor()
      await page.getByRole('switch', { name: /Enable auto save/ }).click()
      await label.scrollIntoViewIfNeeded()
      await page.waitForTimeout(500)
      return page.locator('#settings-section-behavior')
    }
  }
]
