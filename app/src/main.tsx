import { StrictMode, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { listen } from '@tauri-apps/api/event'
import './styles.css'
import MainWindow from './MainWindow'
import LibraryWindow from './LibraryWindow'
import StatsWindow from './StatsWindow'
import type { AppView } from './types'
import { hasTauriInvoke } from './utils'

const NAVIGATE_EVENT = 'app:navigate'

function resolveInitialView(): AppView {
  if (hasTauriInvoke) {
    try {
      const label = getCurrentWebviewWindow().label
      if (label === 'library' || label === 'stats' || label === 'main') {
        return label
      }
    } catch {
      // Fall back to the browser route.
    }
  }

  const params = new URLSearchParams(window.location.search)
  const requestedWindow = params.get('window')

  if (requestedWindow === 'library' || requestedWindow === 'stats' || requestedWindow === 'main') {
    return requestedWindow
  }

  return 'main'
}

function AppRouter() {
  const [currentView, setCurrentView] = useState<AppView>(resolveInitialView)

  useEffect(() => {
    function applyView(nextView: AppView, replace = false) {
      setCurrentView(nextView)

      const url = new URL(window.location.href)
      url.searchParams.set('window', nextView)

      if (replace) {
        window.history.replaceState({ window: nextView }, '', url)
        return
      }

      window.history.pushState({ window: nextView }, '', url)
    }

    applyView(resolveInitialView(), true)

    function handlePopState() {
      setCurrentView(resolveInitialView())
    }

    window.addEventListener('popstate', handlePopState)

    let unlisten: (() => void) | undefined

    void (async () => {
      if (!hasTauriInvoke) {
        return
      }

      unlisten = await listen<AppView>(NAVIGATE_EVENT, (event) => {
        const nextView = event.payload
        if (nextView === 'main' || nextView === 'library' || nextView === 'stats') {
          applyView(nextView)
        }
      })
    })()

    return () => {
      window.removeEventListener('popstate', handlePopState)
      unlisten?.()
    }
  }, [])

  if (currentView === 'library') {
    return <LibraryWindow />
  }

  if (currentView === 'stats') {
    return <StatsWindow />
  }

  return <MainWindow />
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <AppRouter />
  </StrictMode>,
)
