import type { Locator, Page } from 'playwright'
import { DEMO_BASE_URL, DEMO_TOKEN, DEMO_WORKSPACE_ROOT } from '../demo-server'
import type { Shot } from '../shot'
import { openApp, startChat } from '../ui'

interface Box {
  x: number
  y: number
  width: number
  height: number
}

interface Callout extends Box {
  label: string
  /** Where the badge sits inside the box, as fractions of its width and height. */
  at: [number, number]
}

async function boxOf(target: Locator): Promise<Box> {
  const box = await target.boundingBox()
  if (!box) throw new Error(`no bounding box for ${target}`)
  return box
}

/** Draw numbered badges and outlines over regions so a full-window shot can be labeled in prose. */
async function drawCallouts(page: Page, boxes: Callout[]): Promise<void> {
  await page.evaluate((items) => {
    for (const item of items) {
      const outline = document.createElement('div')
      Object.assign(outline.style, {
        position: 'fixed',
        left: `${item.x + 2}px`,
        top: `${item.y + 2}px`,
        width: `${item.width - 4}px`,
        height: `${item.height - 4}px`,
        border: '2px solid #f59e0b',
        borderRadius: '8px',
        zIndex: '99998',
        pointerEvents: 'none'
      })
      const badge = document.createElement('div')
      badge.textContent = item.label
      Object.assign(badge.style, {
        position: 'fixed',
        left: `${item.x + item.width * item.at[0] - 14}px`,
        top: `${item.y + item.height * item.at[1] - 14}px`,
        width: '28px',
        height: '28px',
        borderRadius: '14px',
        background: '#f59e0b',
        color: '#111',
        font: '700 15px/28px system-ui, sans-serif',
        textAlign: 'center',
        zIndex: '99999',
        pointerEvents: 'none',
        boxShadow: '0 1px 4px rgba(0,0,0,.5)'
      })
      document.body.append(outline, badge)
    }
  }, boxes)
}

async function openNewProjectDialog(page: Page): Promise<Locator> {
  await page.getByRole('button', { name: 'Add project or folder group' }).first().click()
  await page.getByRole('menuitem', { name: 'New project' }).click()
  const dialog = page
    .locator('div')
    .filter({ has: page.getByText('Create New Project', { exact: true }) })
  await page.getByText('Create New Project', { exact: true }).waitFor()
  return dialog.last()
}

/** The dialog panel that holds a heading with this exact text. */
function panelWithHeading(page: Page, heading: string): Locator {
  return page
    .getByText(heading, { exact: true })
    .locator('xpath=ancestor::*[contains(@class,"rounded")][1]')
}

const DEMO_PROMPT = 'Add a discount option to the cart total'

/** Shots in this file share one server, so reopen the demo chat when an earlier shot already started it. */
async function openDemoChat(page: Page): Promise<void> {
  await openApp(page)
  const existing = page.getByText(DEMO_PROMPT.slice(0, 20)).first()
  if (await existing.isVisible()) {
    await existing.click()
    await page.waitForTimeout(2_000)
  } else {
    await startChat(page, DEMO_PROMPT)
  }
}

const PALETTE_PLACEHOLDER = 'Search commands, projects, settings...'

/** Open the palette and return its panel: the nearest ancestor of the search box that also holds the footer. */
async function openPalette(page: Page): Promise<Locator> {
  await page.locator('body').click({ position: { x: 640, y: 790 } })
  await page.keyboard.press('Control+k')
  const input = page.getByPlaceholder(PALETTE_PLACEHOLDER)
  await input.waitFor()
  return input.locator('xpath=ancestor::*[.//*[normalize-space(text())="Close"]][1]')
}

export const shots: Shot[] = [
  {
    id: 'overview-hero',
    run: async (page) => {
      await openDemoChat(page)
    }
  },
  {
    id: 'getting-started-window',
    run: async (page) => {
      await openDemoChat(page)
      const rail = await boxOf(page.getByTestId('shell-rail'))
      const sidePanel = await boxOf(
        page.getByRole('tablist', { name: 'Side panel' }).locator('xpath=../..')
      )
      const newTab = await boxOf(page.getByRole('button', { name: 'New tab' }).first())
      const status = await boxOf(page.getByRole('button', { name: 'Context bar settings' }))
      const left = sidePanel.x + sidePanel.width + 8
      const right = newTab.x + newTab.width + 12
      const tabBar = { x: left, y: newTab.y - 10, width: right - left, height: newTab.height + 20 }
      const statusBar = {
        x: left,
        y: status.y - 6,
        width: right - left,
        height: status.height + 12
      }
      const mainTop = tabBar.y + tabBar.height
      const main = { x: left, y: mainTop, width: right - left, height: statusBar.y - mainTop }
      await drawCallouts(page, [
        { label: '1', ...rail, at: [0.5, 0.55] },
        { label: '2', ...sidePanel, at: [0.5, 0.55] },
        { label: '3', ...tabBar, at: [0.55, 0.5] },
        { label: '4', ...main, at: [0.5, 0.45] },
        { label: '5', ...statusBar, at: [0.6, 0.5] }
      ])
    }
  },
  {
    id: 'getting-started-new-project',
    run: async (page) => {
      await openApp(page)
      await openNewProjectDialog(page)
      await page.getByPlaceholder('No directory selected').fill(`${DEMO_WORKSPACE_ROOT}/storefront`)
      await page.getByText('Advanced options').click()
      await page.waitForTimeout(400)
      return panelWithHeading(page, 'Create New Project')
    }
  },
  {
    id: 'getting-started-folder-picker',
    run: async (page) => {
      await openApp(page)
      await openNewProjectDialog(page)
      await page.getByRole('button', { name: 'Browse' }).click()
      await page.getByText('Select Project Folder', { exact: true }).waitFor()
      for (const segment of DEMO_WORKSPACE_ROOT.split('/').filter(Boolean)) {
        await page.getByRole('button', { name: segment, exact: true }).first().click()
        await page.waitForTimeout(400)
      }
      return panelWithHeading(page, 'Select Project Folder')
    }
  },
  {
    id: 'getting-started-new-tab-menu',
    run: async (page) => {
      await openApp(page)
      await page.getByRole('button', { name: 'New tab' }).first().click()
      const menu = page.getByRole('menu')
      await menu.waitFor()
      return menu
    }
  },
  {
    id: 'getting-started-project-menu',
    run: async (page) => {
      await openApp(page)
      await page.locator('[aria-label^="Project: payments-api"]').first().click({ button: 'right' })
      const menu = page.getByRole('menu')
      await menu.waitFor()
      return menu
    }
  },
  {
    id: 'getting-started-groups',
    run: async (page) => {
      await openApp(page)
      await page.getByRole('button', { name: 'Add project or folder group' }).first().click()
      await page.getByRole('menuitem', { name: 'New folder group' }).click()
      await page.getByPlaceholder('Enter group name...').fill('Client work')
      await page.getByRole('button', { name: 'Create', exact: true }).click()
      await page.waitForTimeout(500)
      for (const name of ['payments-api', 'acme-shop']) {
        await page.locator(`[aria-label^="Project: ${name}"]`).first().click({ button: 'right' })
        await page.getByRole('menuitem', { name: 'Move to Group' }).hover()
        await page.getByRole('menuitemradio', { name: 'Client work' }).click()
        await page.waitForTimeout(400)
      }
      await page.locator('[aria-label^="Project: blog"]').first().click({ button: 'right' })
      await page.getByRole('menuitem', { name: 'Archive' }).click()
      await page.waitForTimeout(500)
      await page.getByRole('button', { name: /^Archived projects/ }).click()
      await page.waitForTimeout(400)
      return page.getByTestId('shell-rail')
    }
  },
  {
    id: 'getting-started-command-palette',
    run: async (page) => {
      await openApp(page)
      return openPalette(page)
    }
  },
  {
    id: 'getting-started-palette-search',
    run: async (page) => {
      await openApp(page)
      const panel = await openPalette(page)
      await page.getByPlaceholder(PALETTE_PLACEHOLDER).fill('history')
      await page.waitForTimeout(400)
      return panel
    }
  },
  {
    id: 'getting-started-shortcuts-settings',
    run: async (page) => {
      await openApp(page)
      await page.locator('body').click({ position: { x: 640, y: 790 } })
      await page.keyboard.press('Alt+Comma')
      await page
        .locator('[data-category-button="true"]')
        .filter({ hasText: 'Keyboard Shortcuts' })
        .click()
      await page.waitForTimeout(600)
      return panelWithHeading(page, 'Application Preferences')
    }
  },
  {
    id: 'overview-phone',
    viewport: { width: 390, height: 844 },
    run: async (page) => {
      await page.goto(`${DEMO_BASE_URL}/#token=${DEMO_TOKEN}`)
      await page.waitForFunction(
        (t) => localStorage.getItem('termul.webAuthToken') === t,
        DEMO_TOKEN
      )
      await page.goto(`${DEMO_BASE_URL}/#/`)
      await page.getByText('acme-shop · main').waitFor()
      await page
        .getByRole('button', { name: /Demo Agent/ })
        .first()
        .waitFor()
      await page.getByLabel('Agent prompt').first().click()
      await page.keyboard.type(DEMO_PROMPT)
      await page.keyboard.press('Enter')
      await page.waitForTimeout(5000)
    }
  }
]
