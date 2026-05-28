import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import { AppShell } from './AppShell'
import type { FolderRecord, LibraryStats } from './types'
import {
  formatCount,
  formatError,
  formatFolderCount,
  formatIndexedAt,
  formatRelativeTime,
  hasTauriInvoke,
  openView,
  statusTone,
} from './utils'

export default function StatsWindow() {
  const [folders, setFolders] = useState<FolderRecord[]>([])
  const [stats, setStats] = useState<LibraryStats | null>(null)
  const [status, setStatus] = useState('Loading corpus statistics...')
  const [busy, setBusy] = useState(false)

  const tone = statusTone(status, busy)

  async function loadStats() {
    if (!hasTauriInvoke) {
      setFolders([])
      setStats(null)
      return null
    }

    const [nextFolders, nextStats] = await Promise.all([
      invoke<FolderRecord[]>('list_document_folders'),
      invoke<LibraryStats>('get_document_library_stats'),
    ])

    setFolders(nextFolders)
    setStats(nextStats)

    return { nextFolders, nextStats }
  }

  useEffect(() => {
    let cancelled = false

    async function run() {
      try {
        if (!hasTauriInvoke) {
          setFolders([])
          setStats(null)
          setStatus('Browser preview only. Stats update in the desktop app.')
          return
        }

        setBusy(true)

        const [nextFolders, nextStats] = await Promise.all([
          invoke<FolderRecord[]>('list_document_folders'),
          invoke<LibraryStats>('get_document_library_stats'),
        ])

        if (cancelled) {
          return
        }

        setFolders(nextFolders)
        setStats(nextStats)
        setStatus(
          nextStats.documentCount === 0
            ? 'Add and index a folder in Library to start building search coverage.'
            : `Library coverage across ${formatFolderCount(nextStats.folderCount)}.`,
        )
      } catch (error) {
        if (!cancelled) {
          setStatus(formatError(error))
        }
      } finally {
        if (!cancelled) {
          setBusy(false)
        }
      }
    }

    void run()

    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    function handleWindowFocus() {
      void handleRefresh()
    }

    window.addEventListener('focus', handleWindowFocus)

    return () => {
      window.removeEventListener('focus', handleWindowFocus)
    }
  }, [])

  async function handleRefresh() {
    try {
      setBusy(true)

      const next = await loadStats()

      if (!hasTauriInvoke) {
        setStatus('Browser preview only. Stats update in the desktop app.')
        return
      }

      setStatus(
        next === null || next.nextStats.documentCount === 0
          ? 'Stats refreshed. Add and index a folder in Library to start building search coverage.'
          : 'Stats refreshed.',
      )
    } catch (error) {
      setStatus(formatError(error))
    } finally {
      setBusy(false)
    }
  }

  async function handleOpenLibrary() {
    try {
      await openView('library', 'stats')
    } catch (error) {
      setStatus(formatError(error))
    }
  }

  const metrics = [
    {
      label: 'Documents',
      value: formatCount(stats?.documentCount ?? 0),
      detail: 'Indexed files',
    },
    {
      label: 'Passages',
      value: formatCount(stats?.chunkCount ?? 0),
      detail: 'Searchable chunks',
    },
    {
      label: 'Folders',
      value: formatCount(stats?.folderCount ?? 0),
      detail: `${formatCount(stats?.indexedFolderCount ?? 0)} indexed`,
    },
    {
      label: 'Freshness',
      value: formatRelativeTime(stats?.lastIndexedAt ?? null),
      detail: formatIndexedAt(stats?.lastIndexedAt ?? null),
    },
  ]

  return (
    <AppShell currentView="stats">
      <section className="window-layout">
        <header className="window-toolbar window-toolbar-support">
          <div className="toolbar-headline">
            <h1>Stats</h1>
            <p className={`toolbar-copy toolbar-copy-${tone}`}>{status}</p>
          </div>

          <div className="window-toolbar-actions">
            <button type="button" className="secondary-button" onClick={() => void handleRefresh()} disabled={busy}>
              Refresh
            </button>
            <button type="button" className="secondary-button" onClick={() => void handleOpenLibrary()}>
              Open Library
            </button>
          </div>
        </header>

        <section className="window-grid window-grid-stats">
          <section className="section-heading section-heading-inline">
            <div>
              <h2>Search Coverage</h2>
              <p className="helper-text">
                Use this view to confirm how much content is indexed and when the library was last refreshed.
              </p>
            </div>

            <div className="result-tags">
              <span>{formatCount(stats?.indexedFolderCount ?? 0)} indexed folders</span>
              <span>{formatCount((stats?.folderCount ?? 0) - (stats?.indexedFolderCount ?? 0))} pending</span>
            </div>
          </section>

          <section className="stats-grid">
            {metrics.map((metric) => (
              <article className="metric-card" key={metric.label}>
                <p className="metric-label">{metric.label}</p>
                <strong>{metric.value}</strong>
                <span>{metric.detail}</span>
              </article>
            ))}
          </section>

          <section className="surface panel-section">
            <div className="section-heading">
              <div className="section-header compact">
                <h2>Folders</h2>
                <p className="section-caption">{formatFolderCount(folders.length)}</p>
              </div>
              <p className="helper-text">
                Review which sources are ready to search and which still need a first index run.
              </p>
            </div>

            {folders.length === 0 ? (
              <div className="empty-state">
                <h3>No folders yet</h3>
                <p>Open Library to add a local folder and build your first index.</p>
                <div className="empty-state-actions">
                  <button type="button" className="primary-button" onClick={() => void handleOpenLibrary()}>
                    Open Library
                  </button>
                </div>
              </div>
            ) : (
              <div className="folder-list compact-list">
                {folders.map((folder) => {
                  const isIndexed = folder.lastIndexedAt !== null

                  return (
                    <article className="folder-card compact-card" key={folder.id}>
                      <div className="folder-card-top">
                        <div>
                          <p className="result-kicker">{folder.displayName}</p>
                          <h3>{isIndexed ? 'Indexed source' : 'Waiting for first index'}</h3>
                        </div>
                        <span className={`folder-badge ${isIndexed ? 'folder-badge-ready' : 'folder-badge-waiting'}`}>
                          {isIndexed ? 'Ready' : 'Pending'}
                        </span>
                      </div>

                      <code className="folder-path-display" title={folder.path}>
                        {folder.path}
                      </code>

                      <div className="result-tags">
                        <span>{formatIndexedAt(folder.lastIndexedAt)}</span>
                        <span>{folder.watchEnabled ? 'Watch enabled' : 'Manual indexing'}</span>
                      </div>
                    </article>
                  )
                })}
              </div>
            )}
          </section>
        </section>
      </section>
    </AppShell>
  )
}
