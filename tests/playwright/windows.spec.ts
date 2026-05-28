import { expect, test } from '@playwright/test'

test.describe('browser window smoke tests', () => {
  test('main window renders focused search shell', async ({ page }) => {
    await page.goto('/?window=main')

    const primaryNav = page.getByRole('navigation', { name: 'Primary' })
    const searchComposer = page.locator('form.search-composer')

    await expect(page.getByLabel('Compas Docs')).toBeVisible()
    await expect(primaryNav.getByRole('button', { name: 'Search', exact: true })).toHaveAttribute('aria-current', 'page')
    await expect(page.getByRole('textbox', { name: 'Search' })).toBeVisible()
    await expect(searchComposer.getByRole('button', { name: 'Search' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Results' })).toBeVisible()
  })

  test('library window renders folder management UI', async ({ page }) => {
    await page.goto('/?window=library')

    await expect(page.getByRole('heading', { name: 'Library' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Add folder' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Add Folder' })).toBeVisible()
  })

  test('stats window renders corpus overview cards', async ({ page }) => {
    await page.goto('/?window=stats')

    await expect(page.getByRole('heading', { name: 'Stats' })).toBeVisible()
    await expect(page.getByText('Documents')).toBeVisible()
    await expect(page.getByText('Passages')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Folders', exact: true })).toBeVisible()
  })
})
