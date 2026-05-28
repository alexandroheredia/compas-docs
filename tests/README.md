# UI Smoke Tests

## Browser smoke tests

These use fully local Playwright against the Vite app with browser fallback routing:

- `/?window=main`
- `/?window=library`
- `/?window=stats`

Run:

```bash
npm run playwright:install
npm run test:ui
```
