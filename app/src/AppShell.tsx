import './styles.css'
import type { ReactNode } from 'react'
import type { AppView } from './types'
import { openView } from './utils'

type AppShellProps = {
  currentView: AppView
  children: ReactNode
}

const NAV_ITEMS: Array<{ view: AppView; label: string }> = [
  { view: 'main', label: 'Search' },
  { view: 'library', label: 'Library' },
  { view: 'stats', label: 'Stats' },
]

export function AppShell({ currentView, children }: AppShellProps) {
  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="app-header-inner">
          <div className="app-brand" aria-label="Compas Docs">
            <span>Compas</span>
            <span>Docs</span>
          </div>

          <nav className="app-nav" aria-label="Primary">
            {NAV_ITEMS.map((item) => (
              <button
                key={item.view}
                type="button"
                className={`nav-item ${item.view === currentView ? 'is-active' : ''}`}
                aria-current={item.view === currentView ? 'page' : undefined}
                onClick={() => void openView(item.view, currentView)}
              >
                <span className="nav-icon" aria-hidden="true">
                  {item.view === 'main' ? <SearchIcon /> : item.view === 'library' ? <LibraryIcon /> : <StatsIcon />}
                </span>
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </div>
      </header>

      <section className="app-panel">{children}</section>
    </main>
  )
}

function SearchIcon() {
  return (
    <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.8">
      <circle cx="8.5" cy="8.5" r="5.5" />
      <path d="M12.5 12.5 17 17" strokeLinecap="round" />
    </svg>
  )
}

function LibraryIcon() {
  return (
    <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M3 5.25A2.25 2.25 0 0 1 5.25 3h2.3a2 2 0 0 1 1.4.57l.97.93H14.75A2.25 2.25 0 0 1 17 6.75v7A2.25 2.25 0 0 1 14.75 16H5.25A2.25 2.25 0 0 1 3 13.75v-8.5Z" strokeLinejoin="round" />
      <path d="M3 7h14" strokeLinecap="round" />
    </svg>
  )
}

function StatsIcon() {
  return (
    <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M4 16V9" strokeLinecap="round" />
      <path d="M10 16V5" strokeLinecap="round" />
      <path d="M16 16v-3" strokeLinecap="round" />
      <path d="M3 16.25h14" strokeLinecap="round" />
    </svg>
  )
}
