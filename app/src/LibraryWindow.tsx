import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { AppShell } from './AppShell'
import type { FolderRecord, RemoveFolderResponse } from './types'
import {
  formatCount,
  formatError,
  formatFolderCount,
  formatIndexedAt,
  hasTauriInvoke,
  openView,
  statusTone,
} from './utils'

type BusyAction =
  | { type: 'add' }
  | { type: 'index'; folderId: string }
  | { type: 'remove'; folderId: string }
  | null

export default function LibraryWindow() {
  const [folders, setFolders] = useState<FolderRecord[]>([])
  const [folderPath, setFolderPath] = useState('')
  const [status, setStatus] = useState('Loading folders...')
  const [busyAction, setBusyAction] = useState<BusyAction>(null)
  const folderPathRef = useRef<HTMLInputElement>(null)

  const indexedFolderCount = folders.filter((folder) => folder.lastIndexedAt !== null).length
  const pendingFolderCount = folders.length - indexedFolderCount
  const busy = busyAction !== null
  const tone = statusTone(status, busy)

  function describeLibraryStatus(nextFolders: FolderRecord[]) {
    if (!hasTauriInvoke) {
      return 'Browser preview only. Folder management runs in the desktop app.'
    }

    const nextIndexedFolderCount = nextFolders.filter((folder) => folder.lastIndexedAt !== null).length
    const nextPendingFolderCount = nextFolders.length - nextIndexedFolderCount

    if (nextFolders.length === 0) {
      return 'Add a local folder to create your first searchable library.'
    }

    if (nextIndexedFolderCount === 0) {
      return `${formatFolderCount(nextFolders.length)} added. Run the first index to make ${nextFolders.length === 1 ? 'it' : 'them'} searchable.`
    }

    if (nextPendingFolderCount === 0) {
      return `${formatFolderCount(nextIndexedFolderCount)} indexed and ready to search.`
    }

    return `${formatFolderCount(nextIndexedFolderCount)} indexed. ${formatFolderCount(nextPendingFolderCount)} still ${nextPendingFolderCount === 1 ? 'needs' : 'need'} a first index.`
  }

  function focusPathInput() {
    folderPathRef.current?.focus()
    folderPathRef.current?.select()
  }

  async function loadFolders() {
    if (!hasTauriInvoke) {
      setFolders([])
      return []
    }

    const nextFolders = await invoke<FolderRecord[]>('list_document_folders')
    setFolders(nextFolders)
    return nextFolders
  }

  useEffect(() => {
    let cancelled = false

    async function run() {
      try {
        const nextFolders = await loadFolders()
        if (!cancelled) {
          setStatus(describeLibraryStatus(nextFolders))
        }
      } catch (error) {
        if (!cancelled) {
          setStatus(formatError(error))
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
      void (async () => {
        try {
          const nextFolders = await loadFolders()
          setStatus(describeLibraryStatus(nextFolders))
        } catch (error) {
          setStatus(formatError(error))
        }
      })()
    }

    window.addEventListener('focus', handleWindowFocus)

    return () => {
      window.removeEventListener('focus', handleWindowFocus)
    }
  }, [])

  async function handleRefresh() {
    try {
      const nextFolders = await loadFolders()
      setStatus(describeLibraryStatus(nextFolders))
    } catch (error) {
      setStatus(formatError(error))
    }
  }

  async function handleOpenSearch() {
    try {
      await openView('main', 'library')
    } catch (error) {
      setStatus(formatError(error))
    }
  }

  async function handleAddFolder(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const trimmedPath = folderPath.trim()
    if (!trimmedPath) {
      setStatus('Paste the full local folder path first.')
      focusPathInput()
      return
    }

    if (!hasTauriInvoke) {
      setStatus('Folder management runs in the desktop app.')
      return
    }

    setBusyAction({ type: 'add' })
    setStatus(`Adding ${trimmedPath}...`)

    try {
      const record = await invoke<FolderRecord>('add_document_folder', { path: trimmedPath })
      setFolderPath('')
      await loadFolders()
      setStatus(`Added ${record.displayName}. Run its first index when you are ready.`)
    } catch (error) {
      setStatus(formatError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleIndexFolder(folder: FolderRecord) {
    if (!hasTauriInvoke) {
      return
    }

    setBusyAction({ type: 'index', folderId: folder.id })
    setStatus(`Indexing ${folder.displayName}...`)

    try {
      await invoke<FolderRecord>('index_document_folder', { path: folder.path })
      const nextFolders = await loadFolders()
      setStatus(
        nextFolders.some((record) => record.id === folder.id && record.lastIndexedAt !== null)
          ? `${folder.displayName} is ready to search.`
          : `Index finished for ${folder.displayName}.`,
      )
    } catch (error) {
      setStatus(formatError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleRemoveFolder(folder: FolderRecord) {
    if (!hasTauriInvoke) {
      return
    }

    const confirmed = window.confirm(
      `Remove "${folder.displayName}" from the library?\n\nThis keeps the source folder on disk, but deletes its local search index.`,
    )

    if (!confirmed) {
      return
    }

    setBusyAction({ type: 'remove', folderId: folder.id })
    setStatus(`Removing ${folder.displayName}...`)

    try {
      const response = await invoke<RemoveFolderResponse>('remove_document_folder', {
        id: folder.id,
      })

      const nextFolders = await loadFolders()
      setStatus(
        response.removed
          ? `${folder.displayName} removed. The source folder was left in place.`
          : `${folder.displayName} was already missing. ${describeLibraryStatus(nextFolders)}`,
      )
    } catch (error) {
      setStatus(formatError(error))
    } finally {
      setBusyAction(null)
    }
  }

  return (
    <AppShell currentView="library">
      <section className="window-layout">
        <header className="window-toolbar window-toolbar-support">
          <div className="toolbar-headline">
            <h1>Library</h1>
            <p className={`toolbar-copy toolbar-copy-${tone}`}>{status}</p>
          </div>

          <div className="window-toolbar-actions">
            <button type="button" className="secondary-button" onClick={() => void handleRefresh()} disabled={busy}>
              Refresh
            </button>
            <button type="button" className="secondary-button" onClick={() => void handleOpenSearch()}>
              Open Search
            </button>
          </div>
        </header>

        <section className="window-grid window-grid-library">
          <section className="surface panel-section">
            <div className="section-heading">
              <div className="section-header compact">
                <h2>Add Folder</h2>
                <p className="section-caption">1. Add 2. Index 3. Search</p>
              </div>
              <p className="helper-text">
                Paste a local folder path. Compas Docs keeps the source files in place and builds a separate search index.
              </p>
            </div>

            <form className="stack-form" onSubmit={handleAddFolder}>
              <label className="stack-field" htmlFor="folder-path">
                <span>Folder path</span>
                <input
                  id="folder-path"
                  ref={folderPathRef}
                  value={folderPath}
                  onChange={(event) => setFolderPath(event.target.value)}
                  disabled={!hasTauriInvoke || busy}
                  placeholder="/Users/alexandro/Documents/Contracts"
                  spellCheck={false}
                />
              </label>

              <p className="helper-text helper-text-code">
                Example: <code>/Users/alexandro/Documents/Contracts</code>
              </p>

              <button type="submit" className="primary-button" disabled={!hasTauriInvoke || busy}>
                {busyAction?.type === 'add' ? 'Adding...' : 'Add folder'}
              </button>
            </form>

            <p className="panel-note">
              {!hasTauriInvoke
                ? 'Browser preview only. Add, index, and remove folders in the desktop app.'
                : 'After adding a folder, run its first index to make it searchable from the main Search window.'}
            </p>
          </section>

          <section className="surface panel-section">
            <div className="section-heading">
              <div className="section-header compact">
                <h2>Folders</h2>
                <p className="section-caption">{formatFolderCount(folders.length)}</p>
              </div>

              <div className="result-tags">
                <span>{formatCount(indexedFolderCount)} ready</span>
                <span>{formatCount(pendingFolderCount)} pending</span>
              </div>
            </div>

            {folders.length === 0 ? (
              <div className="empty-state">
                <h3>No folders yet</h3>
                <p>Add a local folder to create your first searchable library.</p>
                <div className="empty-state-actions">
                  {hasTauriInvoke ? (
                    <button type="button" className="primary-button" onClick={focusPathInput}>
                      Focus path field
                    </button>
                  ) : null}
                  <button type="button" className="secondary-button" onClick={() => void handleOpenSearch()}>
                    Open Search
                  </button>
                </div>
              </div>
            ) : (
              <div className="folder-list">
                {folders.map((folder) => {
                  const isIndexed = folder.lastIndexedAt !== null
                  const isIndexing = busyAction?.type === 'index' && busyAction.folderId === folder.id
                  const isRemoving = busyAction?.type === 'remove' && busyAction.folderId === folder.id

                  return (
                    <article className="folder-card" key={folder.id}>
                      <div className="folder-card-top">
                        <div>
                          <p className="result-kicker">{isIndexed ? 'Indexed source' : 'Waiting for first index'}</p>
                          <h3>{folder.displayName}</h3>
                        </div>
                        <span className={`folder-badge ${isIndexed ? 'folder-badge-ready' : 'folder-badge-waiting'}`}>
                          {isIndexed ? 'Ready' : 'Needs index'}
                        </span>
                      </div>

                      <code className="folder-path-display" title={folder.path}>
                        {folder.path}
                      </code>

                      <div className="result-tags">
                        <span>{formatIndexedAt(folder.lastIndexedAt)}</span>
                        <span>{folder.watchEnabled ? 'Watching enabled' : 'Manual indexing'}</span>
                      </div>

                      <div className="result-actions folder-card-actions">
                        <button
                          type="button"
                          className="secondary-button"
                          disabled={busy}
                          onClick={() => void handleIndexFolder(folder)}
                        >
                          {isIndexing ? 'Indexing...' : isIndexed ? 'Reindex' : 'Run first index'}
                        </button>
                        <button
                          type="button"
                          className="secondary-button danger"
                          disabled={busy}
                          onClick={() => void handleRemoveFolder(folder)}
                        >
                          {isRemoving ? 'Removing...' : 'Remove'}
                        </button>
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
