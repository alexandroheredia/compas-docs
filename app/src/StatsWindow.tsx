import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'
import { AppShell } from './AppShell'
import { ToastStack, useToasts } from './Toast'
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
  const [status, setStatus] = useState('Loading library statistics…')
  const [busy, setBusy] = useState(false)
  // Prevent the window-focus refresh from clobbering a freshly surfaced
  // error or a manual-refresh confirmation.
  const lastManualActionRef = useRef(0)
  const { toasts, push: pushToast, dismiss: dismissToast } = useToasts()

  const tone = statusTone(status, busy)
  const hasData = stats !== null && stats.documentCount > 0

  function describeStats(next: LibraryStats | null) {
    if (!hasTauriInvoke) return 'Browser preview. Stats update in the desktop app.'
    if (!next) return 'No data yet.'
    if (next.documentCount === 0) return 'Add and index a folder in Library to start building coverage.'
    return `Coverage across ${formatFolderCount(next.folderCount)}.`
  }

  async function loadAll() {
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
    return nextStats
  }

  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        setBusy(true)
        const nextStats = await loadAll()
        if (cancelled) return
        setStatus(describeStats(nextStats))
      } catch (error) {
        if (!cancelled) setStatus(formatError(error))
      } finally {
        if (!cancelled) setBusy(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    function handleFocus() {
      void (async () => {
        // Throttle: only refresh if more than 800ms passed since the last
        // manual action, so the manual-refresh status message stays visible.
        if (Date.now() - lastManualActionRef.current < 800) return
        try {
          await loadAll()
        } catch {
          // Silent on background refresh failure.
        }
      })()
    }
    window.addEventListener('focus', handleFocus)
    return () => window.removeEventListener('focus', handleFocus)
  }, [])

  async function handleRefresh() {
    lastManualActionRef.current = Date.now()
    setBusy(true)
    try {
      const nextStats = await loadAll()
      setStatus(describeStats(nextStats))
    } catch (error) {
      setStatus(formatError(error))
      pushToast('error', formatError(error))
    } finally {
      setBusy(false)
    }
  }

  const indexedCount = stats?.indexedFolderCount ?? 0
  const pendingCount = Math.max(0, (stats?.folderCount ?? 0) - indexedCount)

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
      detail: `${formatCount(indexedCount)} indexed · ${formatCount(pendingCount)} pending`,
    },
    {
      label: 'Last index',
      value: formatRelativeTime(stats?.lastIndexedAt ?? null),
      detail: formatIndexedAt(stats?.lastIndexedAt ?? null),
    },
  ]

  return (
    <AppShell currentView="stats">
      <div className="window">
        <header className="page-header">
          <div className="page-header-title">
            <h1>Stats</h1>
            <p className={`page-header-sub${tone !== 'idle' ? ` status-${tone}` : ''}`}>{status}</p>
          </div>
          <div className="page-header-actions">
            <button type="button" className="btn btn-secondary" onClick={() => void handleRefresh()} disabled={busy}>
              Refresh
            </button>
            <button type="button" className="btn btn-secondary" onClick={() => void openView('library', 'stats')}>
              Open Library
            </button>
          </div>
        </header>

        <section className="stats-grid" aria-label="Library metrics">
          {metrics.map((metric) => (
            <article className="metric-card" key={metric.label}>
              <span className="metric-label">{metric.label}</span>
              <span className="metric-value">{busy && !hasData ? '—' : metric.value}</span>
              <span className="metric-detail">{metric.detail}</span>
            </article>
          ))}
        </section>

        <section className="surface section" aria-labelledby="folder-overview-heading">
          <div className="section-head">
            <h2 id="folder-overview-heading">Folders</h2>
            <span className="section-meta">{formatFolderCount(folders.length)}</span>
          </div>

          {folders.length === 0 ? (
            <div className="empty-state">
              <h3>No folders yet</h3>
              <p>Open Library to add a local folder and build your first index.</p>
              <div className="empty-state-actions">
                <button type="button" className="btn btn-primary" onClick={() => void openView('library', 'stats')}>
                  Open Library
                </button>
              </div>
            </div>
          ) : (
            <div className="folder-list">
              {folders.map((folder) => {
                const isIndexed = folder.lastIndexedAt !== null
                return (
                  <article className="folder-card" key={folder.id}>
                    <div className="folder-card-top">
                      <div className="folder-card-title">
                        <h3>{folder.displayName}</h3>
                        <code className="folder-path" title={folder.path}>{folder.path}</code>
                      </div>
                      {isIndexed ? (
                        <span className="badge badge-success"><span className="badge-dot" />Ready</span>
                      ) : (
                        <span className="badge badge-warn"><span className="badge-dot" />Pending</span>
                      )}
                    </div>
                    <div className="folder-meta">
                      <span>{formatIndexedAt(folder.lastIndexedAt)}</span>
                      <span>{folder.watchEnabled ? 'Watch enabled' : 'Manual indexing'}</span>
                      <span>{folder.fileTypes.map((t) => t.toUpperCase()).join(' · ') || 'No file types'}</span>
                    </div>
                  </article>
                )
              })}
            </div>
          )}
        </section>
      </div>
      <ToastStack toasts={toasts} onDismiss={dismissToast} />
    </AppShell>
  )
}
