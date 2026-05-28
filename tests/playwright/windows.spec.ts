import { expect, test } from '@playwright/test'

test.describe('browser window smoke tests', () => {
  test('main window renders search shell + composer', async ({ page }) => {
    await page.goto('/?window=main')

    const primaryNav = page.getByRole('navigation', { name: 'Primary' })

    await expect(page.getByLabel('Compas Docs')).toBeVisible()
    await expect(primaryNav.getByRole('button', { name: 'Search' })).toHaveAttribute('aria-current', 'page')
    await expect(page.getByRole('heading', { name: 'Search', level: 1 })).toBeVisible()
    await expect(page.getByRole('textbox', { name: 'Search' })).toBeVisible()
    await expect(page.getByRole('search').getByRole('button', { name: 'Search' })).toBeVisible()
  })

  test('library window renders folder management UI', async ({ page }) => {
    await page.goto('/?window=library')

    await expect(page.getByRole('heading', { name: 'Library', level: 1 })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Add folder' })).toBeVisible()
    await expect(page.getByLabel('Folder path')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Add folder' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Choose…' })).toBeVisible()
  })

  test('stats window renders metric cards', async ({ page }) => {
    await page.goto('/?window=stats')

    await expect(page.getByRole('heading', { name: 'Stats', level: 1 })).toBeVisible()
    await expect(page.getByText('Documents', { exact: true })).toBeVisible()
    await expect(page.getByText('Passages', { exact: true })).toBeVisible()
    await expect(page.getByText('Last index', { exact: true })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Folders', level: 2 })).toBeVisible()
  })
})
