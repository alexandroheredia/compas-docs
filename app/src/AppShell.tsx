import './styles.css'
import type { ReactNode } from 'react'
import type { AppView } from './types'
import { openView } from './utils'

type AppShellProps = {
  currentView: AppView
  children: ReactNode
}

// Each nav item carries its own icon glyph so the rail stays compact at narrow widths
// (label collapses to icon below 600px, see styles.css).
const NAV_ITEMS: Array<{ view: AppView; label: string; icon: ReactNode }> = [
  { view: 'main', label: 'Search', icon: <SearchIcon /> },
  { view: 'library', label: 'Library', icon: <LibraryIcon /> },
  { view: 'stats', label: 'Stats', icon: <StatsIcon /> },
]

export function AppShell({ currentView, children }: AppShellProps) {
  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="app-header-inner">
          <div className="app-brand" aria-label="Compas Docs">
            {/* Compact gradient mark keeps the brand legible without dominating the header. */}
            <span className="app-brand-mark" aria-hidden="true">C</span>
            <span>Compas</span>
            <span className="app-brand-sub">Docs</span>
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
                <span className="nav-icon" aria-hidden="true">{item.icon}</span>
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
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <circle cx="7" cy="7" r="4.5" />
      <path d="m10.5 10.5 3 3" />
    </svg>
  )
}

function LibraryIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round">
      <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.4a1.4 1.4 0 0 1 .98.4l.84.8H12.5A1.5 1.5 0 0 1 14 5.7v6.3A1.5 1.5 0 0 1 12.5 13.5h-9A1.5 1.5 0 0 1 2 12V4.5Z" />
    </svg>
  )
}

function StatsIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <path d="M3 13V7.5" />
      <path d="M8 13V3.5" />
      <path d="M13 13v-4" />
    </svg>
  )
}
