import { execSync } from 'node:child_process'
import type { Page } from 'playwright'
import { DEMO_BASE_URL, DEMO_TOKEN } from '../demo-server'
import type { Shot } from '../shot'
import { openApp, startChat } from '../ui'

const phone = { width: 390, height: 844 }

async function openPhone(page: Page): Promise<void> {
  await page.goto(`${DEMO_BASE_URL}/#token=${DEMO_TOKEN}`)
  await page.waitForFunction((t) => localStorage.getItem('termul.webAuthToken') === t, DEMO_TOKEN)
  await page.goto(`${DEMO_BASE_URL}/#/`)
  await page.getByRole('button', { name: 'Open menu' }).waitFor()
  await page.locator('[data-composer-editor="true"][aria-label="Agent prompt"]').first().waitFor()
  await page.waitForTimeout(1_000)
}

async function phoneChat(page: Page, prompt: string, settleMs = 4_000): Promise<void> {
  await page.locator('[data-composer-editor="true"][aria-label="Agent prompt"]').first().click()
  await page.keyboard.type(prompt)
  await page.getByRole('button', { name: 'Start agent chat' }).click()
  await page.waitForTimeout(settleMs)
}

async function openMore(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'More', exact: true }).click()
  await page.getByRole('dialog').waitFor()
  await page.waitForTimeout(400)
}

/** Stops the demo server the way an operator would, so the app sees its host go away. */
function stopDemoServer(): void {
  const port = new URL(DEMO_BASE_URL).port
  execSync(`pkill -TERM -f "termul-serve[r] --host 127.0.0.1 --port ${port} "`)
}

export const shots: Shot[] = [
  {
    id: 'remote-token-screen',
    run: async (page) => {
      await page.goto(`${DEMO_BASE_URL}/#/`)
      const form = page.getByRole('form', { name: 'Server access token' })
      await form.waitFor()
      return form
    }
  },
  {
    id: 'remote-browser-connected',
    run: async (page) => {
      await openApp(page)
      await page.getByRole('status', { name: 'Connected' }).getByRole('button').hover()
      await page.getByRole('tooltip').waitFor()
    }
  },
  {
    id: 'remote-phone-chat',
    viewport: phone,
    run: async (page) => {
      await openPhone(page)
      await phoneChat(page, 'Add a discount option to the cart total')
    }
  },
  {
    id: 'remote-phone-drawer',
    viewport: phone,
    run: async (page) => {
      await openPhone(page)
      await phoneChat(page, 'Add a discount option to the cart total')
      await page.getByRole('button', { name: 'Open menu' }).click()
      await page.getByRole('navigation', { name: 'Sections' }).waitFor()
      await page.waitForTimeout(600)
    }
  },
  {
    id: 'remote-phone-more',
    viewport: phone,
    run: async (page) => {
      await openPhone(page)
      await phoneChat(page, 'Add a discount option to the cart total')
      await openMore(page)
    }
  },
  {
    id: 'remote-phone-approval',
    viewport: phone,
    run: async (page) => {
      await openPhone(page)
      await phoneChat(page, 'Run the test suite [ASK]')
    }
  },
  {
    id: 'remote-phone-terminal',
    viewport: phone,
    run: async (page) => {
      await openPhone(page)
      await page.getByRole('button', { name: 'Open menu' }).click()
      await page
        .getByRole('navigation', { name: 'Sections' })
        .getByRole('button', { name: 'Terminals' })
        .click()
      await page.getByRole('button', { name: 'New terminal' }).last().click()
      await page.getByRole('group', { name: 'Terminal keys' }).waitFor()
      await page.waitForTimeout(1_500)
      await page.keyboard.type('git status --short')
      await page.keyboard.press('Enter')
      await page.waitForTimeout(1_500)
    }
  },
  {
    id: 'remote-phone-files',
    viewport: phone,
    run: async (page) => {
      await openPhone(page)
      await phoneChat(page, 'Add a discount option to the cart total')
      await openMore(page)
      await page.getByRole('button', { name: 'Files', exact: true }).click()
      await page.getByRole('list', { name: /^Files in / }).waitFor()
      await page.waitForTimeout(600)
    }
  },
  {
    id: 'remote-phone-git',
    viewport: phone,
    run: async (page) => {
      await openPhone(page)
      await phoneChat(page, 'Add a discount option to the cart total')
      await openMore(page)
      await page.getByRole('button', { name: 'Git changes', exact: true }).click()
      await page.waitForTimeout(2_000)
    }
  },
  {
    id: 'remote-notification-settings',
    run: async (page) => {
      await openApp(page)
      await page.keyboard.press('Alt+Comma')
      await page.getByRole('button', { name: 'Behavior', exact: true }).click()
      const section = page.getByText('Notifications', { exact: true }).locator('..')
      await section.waitFor()
      return section
    }
  },
  {
    // Runs last: it stops this file's demo server.
    id: 'remote-host-offline',
    run: async (page) => {
      await openApp(page, 'payments-api')
      await startChat(page, 'Add a health check route')
      stopDemoServer()
      await page.getByText('Host offline').first().waitFor({ timeout: 20_000 })
      await page.waitForTimeout(800)
    }
  }
]
