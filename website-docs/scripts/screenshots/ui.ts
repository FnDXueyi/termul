import type { Page } from 'playwright'
import { DEMO_BASE_URL, DEMO_TOKEN } from './demo-server'

export async function openApp(page: Page, project = 'acme-shop'): Promise<void> {
  // The first load after a server start occasionally misses the token
  // bootstrap, so retry the fragment visit once.
  for (let attempt = 0; ; attempt++) {
    await page.goto(`${DEMO_BASE_URL}/#token=${DEMO_TOKEN}`)
    try {
      await page.waitForFunction(
        (t) => localStorage.getItem('termul.webAuthToken') === t,
        DEMO_TOKEN,
        {
          timeout: 10_000
        }
      )
      break
    } catch (error) {
      if (attempt > 0) throw error
    }
  }
  await page.goto(`${DEMO_BASE_URL}/#/`)
  await page.locator('[aria-label^="Project: "]').first().waitFor()
  await selectProject(page, project)
}

export async function selectProject(page: Page, name: string): Promise<void> {
  await page.locator(`[aria-label^="Project: ${name}"]`).first().click()
  await page.locator(`[aria-label="Project: ${name} (active)"]`).first().waitFor()
  await page.waitForTimeout(800)
}

/** Start a new chat with the Demo Agent and wait for its reply to finish. */
export async function startChat(page: Page, prompt: string, settleMs = 4_000): Promise<void> {
  await page.getByRole('button', { name: 'New chat', exact: true }).first().click()
  const composer = page.locator('[data-composer-editor="true"][aria-label="Agent prompt"]').first()
  await composer.click()
  await page.keyboard.type(prompt)
  await page.keyboard.press('Enter')
  await page.waitForTimeout(settleMs)
}
