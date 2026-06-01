import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { useEffect, useRef, useState } from 'react'
import { AppShell } from './AppShell'
import { ToastStack, useToasts } from './Toast'
import type {
  FolderRecord,
  IndexProgressEvent,
  RemoveFolderResponse,
  WatchFolderResponse,
  WatchStatusEvent,
} from './types'
import {
  INDEX_PROGRESS_EVENT,
  WATCH_STATUS_EVENT,
  basename,
  formatCount,
  formatError,
  formatFolderCount,
  formatIndexedAt,
  hasTauriInvoke,
  statusTone,
} from './utils'

const SUPPORTED_FILE_TYPES = ['md', 'txt', 'pdf'] as const
const DEFAULT_FILE_TYPES = [...SUPPORTED_FILE_TYPES]

type BusyAction =
  | { type: 'add' }
  | { type: 'pick' }
  | { type: 'index'; folderId: string }
  | { type: 'remove'; folderId: string }
  | null

type FolderProgress = {
  processed: number
  total: number
  currentPath: string | null
  finalizing: boolean
}

type FolderWatchState = {
  phase: WatchStatusEvent['phase']
  path: string | null
  error: string | null
}

export default function LibraryWindow() {
  const [folders, setFolders] = useState<FolderRecord[]>([])
  const [folderPath, setFolderPath] = useState('')
  const [selectedFileTypes, setSelectedFileTypes] = useState<string[]>(DEFAULT_FILE_TYPES)
  const [status, setStatus] = useState('Loading library…')
  const [busyAction, setBusyAction] = useState<BusyAction>(null)
  // Keyed by folder id so concurrent index jobs would render correctly even
  // though the current UI only allows one at a time.
  const [progressMap, setProgressMap] = useState<Map<string, FolderProgress>>(new Map())
  const [watchStateMap, setWatchStateMap] = useState<Map<string, FolderWatchState>>(new Map())
  const folderPathRef = useRef<HTMLInputElement>(null)
  const { toasts, exitingIds, push: pushToast, dismiss: dismissToast } = useToasts()

  const indexedFolderCount = folders.filter((folder) => folder.lastIndexedAt !== null).length
  const pendingFolderCount = folders.length - indexedFolderCount
  const busy = busyAction !== null
  const tone = statusTone(status, busy)

  function describeLibrary(nextFolders: FolderRecord[]) {
    if (!hasTauriInvoke) {
      return 'Browser preview. Folder management runs in the desktop app.'
    }
    const ready = nextFolders.filter((folder) => folder.lastIndexedAt !== null).length
    const pending = nextFolders.length - ready
    if (nextFolders.length === 0) return 'Add a local folder to create your first searchable library.'
    if (ready === 0) {
      return `${formatFolderCount(nextFolders.length)} added · run the first index to make ${nextFolders.length === 1 ? 'it' : 'them'} searchable`
    }
    if (pending === 0) return `${formatFolderCount(ready)} indexed and ready to search.`
    return `${formatFolderCount(ready)} indexed · ${formatFolderCount(pending)} pending`
  }

  async function loadFolders() {
    if (!hasTauriInvoke) {
      setFolders([])
      return []
    }
    const next = await invoke<FolderRecord[]>('list_document_folders')
    setFolders(next)
    return next
  }

  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const next = await loadFolders()
        if (!cancelled) setStatus(describeLibrary(next))
      } catch (error) {
        if (!cancelled) setStatus(formatError(error))
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    // Subscribe to streaming progress events. Each emit updates the per-folder
    // progress map; terminal events (completed/failed) clear it.
    if (!hasTauriInvoke) return
    let unlistenProgress: UnlistenFn | undefined
    let unlistenWatch: UnlistenFn | undefined
    let cancelled = false
    void (async () => {
      unlistenProgress = await listen<IndexProgressEvent>(INDEX_PROGRESS_EVENT, (event) => {
        if (cancelled) return
        const payload = event.payload
        setProgressMap((current) => {
          const next = new Map(current)
          if (payload.phase === 'completed' || payload.phase === 'failed') {
            next.delete(payload.folderId)
            return next
          }
          next.set(payload.folderId, {
            processed: payload.processedFiles,
            total: payload.totalFiles,
            currentPath: payload.currentPath ?? null,
            finalizing: payload.phase === 'finalizing',
          })
          return next
        })
      })

      unlistenWatch = await listen<WatchStatusEvent>(WATCH_STATUS_EVENT, (event) => {
        if (cancelled) return
        const payload = event.payload

        setWatchStateMap((current) => {
          const next = new Map(current)
          if (payload.phase === 'stopped') {
            next.delete(payload.folderId)
            return next
          }
          next.set(payload.folderId, {
            phase: payload.phase,
            path: payload.path ?? null,
            error: payload.error ?? null,
          })
          return next
        })

        if (payload.phase === 'reindex-failed' && payload.error) {
          pushToast('error', payload.error)
        }
      })
    })()
    return () => {
      cancelled = true
      unlistenProgress?.()
      unlistenWatch?.()
    }
  }, [pushToast])

  function focusPathInput() {
    folderPathRef.current?.focus()
    folderPathRef.current?.select()
  }

  function toggleSelection(current: string[], fileType: string) {
    if (current.includes(fileType)) {
      if (current.length === 1) return current
      return current.filter((value) => value !== fileType)
    }
    return SUPPORTED_FILE_TYPES.filter(
      (supported) => supported === fileType || current.includes(supported),
    )
  }

  function toggleSelected(fileType: string) {
    setSelectedFileTypes((current) => toggleSelection(current, fileType))
  }

  async function persistFolderFileTypes(folder: FolderRecord, nextTypes: string[]) {
    // add_document_folder is idempotent and rewrites the registry entry; this
    // is the supported way to persist file-type changes for a folder.
    if (!hasTauriInvoke) return
    try {
      await invoke<FolderRecord>('add_document_folder', {
        path: folder.path,
        fileTypes: nextTypes,
      })
      await loadFolders()
    } catch (error) {
      pushToast('error', formatError(error))
    }
  }

  function toggleFolderFileType(folder: FolderRecord, fileType: string) {
    const next = toggleSelection(folder.fileTypes, fileType)
    if (next === folder.fileTypes) return
    // Optimistic update for snappy feedback, persisted in background.
    setFolders((current) => current.map((f) => (f.id === folder.id ? { ...f, fileTypes: next } : f)))
    void persistFolderFileTypes(folder, next)
  }

  async function handlePickFolder() {
    if (!hasTauriInvoke) return
    setBusyAction({ type: 'pick' })
    try {
      const picked = await invoke<string | null>('pick_document_folder')
      if (picked) {
        setFolderPath(picked)
        focusPathInput()
      }
    } catch (error) {
      pushToast('error', formatError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleAddFolder() {
    const trimmed = folderPath.trim()
    if (!trimmed) {
      setStatus('Choose or paste a local folder path first.')
      focusPathInput()
      return
    }
    if (!hasTauriInvoke) {
      setStatus('Folder management runs in the desktop app.')
      return
    }
    if (selectedFileTypes.length === 0) {
      setStatus('Select at least one file type before adding a folder.')
      return
    }
    setBusyAction({ type: 'add' })
    setStatus(`Adding ${basename(trimmed)}…`)
    try {
      const record = await invoke<FolderRecord>('add_document_folder', {
        path: trimmed,
        fileTypes: selectedFileTypes,
      })
      setFolderPath('')
      setSelectedFileTypes(DEFAULT_FILE_TYPES)
      const next = await loadFolders()
      setStatus(describeLibrary(next))
      pushToast('success', `Added ${record.displayName}. Run its first index when ready.`)
    } catch (error) {
      setStatus(formatError(error))
      pushToast('error', formatError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleIndexFolder(folder: FolderRecord) {
    if (!hasTauriInvoke) return
    if (folder.fileTypes.length === 0) {
      setStatus(`Select at least one file type for ${folder.displayName} before indexing.`)
      return
    }
    setBusyAction({ type: 'index', folderId: folder.id })
    setStatus(`Indexing ${folder.displayName}…`)
    // Seed progress so the bar appears immediately rather than waiting for the
    // first event from the backend.
    setProgressMap((current) => {
      const next = new Map(current)
      next.set(folder.id, { processed: 0, total: 0, currentPath: null, finalizing: false })
      return next
    })
    try {
      await invoke<FolderRecord>('index_document_folder', {
        folderId: folder.id,
        path: folder.path,
        fileTypes: folder.fileTypes,
      })
      const next = await loadFolders()
      const refreshed = next.find((record) => record.id === folder.id)
      setStatus(describeLibrary(next))
      pushToast(
        'success',
        refreshed?.lastIndexedAt
          ? `${folder.displayName} is ready to search.`
          : `Index finished for ${folder.displayName}.`,
      )
    } catch (error) {
      setStatus(formatError(error))
      pushToast('error', `Indexing ${folder.displayName} failed: ${formatError(error)}`)
    } finally {
      setBusyAction(null)
      setProgressMap((current) => {
        const next = new Map(current)
        next.delete(folder.id)
        return next
      })
    }
  }

  async function handleToggleWatch(folder: FolderRecord) {
    if (!hasTauriInvoke) return

    const nextEnabled = !folder.watchEnabled
    setBusyAction({ type: 'index', folderId: folder.id })

    try {
      const response = await invoke<WatchFolderResponse>('set_document_folder_watch_enabled', {
        id: folder.id,
        enabled: nextEnabled,
      })
      setFolders((current) => current.map((item) => (item.id === folder.id ? response.folder : item)))
      setStatus(
        nextEnabled
          ? `Watching ${folder.displayName} for file changes.`
          : `${folder.displayName} is back to manual indexing.`,
      )
      pushToast(
        nextEnabled ? 'success' : 'info',
        nextEnabled
          ? `${folder.displayName} will reindex changed files automatically.`
          : `${folder.displayName} watch disabled.`,
      )
      if (!nextEnabled) {
        setWatchStateMap((current) => {
          const next = new Map(current)
          next.delete(folder.id)
          return next
        })
      }
    } catch (error) {
      setStatus(formatError(error))
      pushToast('error', formatError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleRemoveFolder(folder: FolderRecord) {
    if (!hasTauriInvoke) return
    const confirmed = window.confirm(
      `Remove "${folder.displayName}" from the library?\n\nThe source folder stays on disk; only its search index is deleted.`,
    )
    if (!confirmed) return
    setBusyAction({ type: 'remove', folderId: folder.id })
    setStatus(`Removing ${folder.displayName}…`)
    try {
      const response = await invoke<RemoveFolderResponse>('remove_document_folder', {
        id: folder.id,
      })
      const next = await loadFolders()
      setStatus(describeLibrary(next))
      pushToast(
        response.removed ? 'success' : 'info',
        response.removed
          ? `${folder.displayName} removed.`
          : `${folder.displayName} was already missing.`,
      )
    } catch (error) {
      setStatus(formatError(error))
      pushToast('error', formatError(error))
    } finally {
      setBusyAction(null)
    }
  }

  return (
    <AppShell currentView="library">
      <div className="window">
        <header className="page-header">
          <div className="page-header-title">
            <h1>Library</h1>
            <p className={`page-header-sub${tone !== 'idle' ? ` status-${tone}` : ''}`}>{status}</p>
          </div>
          <div className="page-header-actions">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => void loadFolders().then((next) => setStatus(describeLibrary(next))).catch((error) => setStatus(formatError(error)))}
              disabled={busy}
            >
              Refresh
            </button>
          </div>
        </header>

        <section className="surface section" aria-labelledby="add-folder-heading">
          <div className="section-head">
            <h2 id="add-folder-heading">Add folder</h2>
            <span className="section-meta">Source files stay where they are.</span>
          </div>

          <div className="field">
            <label className="field-label" htmlFor="folder-path">Folder path</label>
            <div className="input-group">
              <input
                id="folder-path"
                ref={folderPathRef}
                value={folderPath}
                onChange={(event) => setFolderPath(event.target.value)}
                disabled={!hasTauriInvoke || busy}
                spellCheck={false}
                placeholder="/Users/you/Documents/Contracts"
              />
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => void handlePickFolder()}
                disabled={!hasTauriInvoke || busy}
                title="Choose a folder"
              >
                {busyAction?.type === 'pick' ? 'Opening…' : 'Choose…'}
              </button>
            </div>
          </div>

          <div className="field">
            <span className="field-label">File types</span>
            <div className="file-type-grid">
              {SUPPORTED_FILE_TYPES.map((fileType) => {
                const checked = selectedFileTypes.includes(fileType)
                const disabled = !hasTauriInvoke || busy || (checked && selectedFileTypes.length === 1)
                return (
                  <label
                    className={`file-type-option${checked ? ' is-selected' : ''}${disabled ? ' is-disabled' : ''}`}
                    key={fileType}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={disabled}
                      onChange={() => toggleSelected(fileType)}
                    />
                    <CheckGlyph />
                    <span>{fileType.toUpperCase()}</span>
                  </label>
                )
              })}
            </div>
          </div>

          <div>
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => void handleAddFolder()}
              disabled={!hasTauriInvoke || busy}
            >
              {busyAction?.type === 'add' ? 'Adding…' : 'Add folder'}
            </button>
          </div>
        </section>

        <section className="surface section" aria-labelledby="folders-heading">
          <div className="section-head">
            <h2 id="folders-heading">Folders</h2>
            <span className="section-meta">
              {formatFolderCount(folders.length)} · {formatCount(indexedFolderCount)} ready · {formatCount(pendingFolderCount)} pending
            </span>
          </div>

          {folders.length === 0 ? (
            <div className="empty-state">
              <h3>No folders yet</h3>
              <p>Add a local folder above to create your first searchable library.</p>
            </div>
          ) : (
            <div className="folder-list">
              {folders.map((folder) => {
                const isIndexing = busyAction?.type === 'index' && busyAction.folderId === folder.id
                const isRemoving = busyAction?.type === 'remove' && busyAction.folderId === folder.id
                const progress = progressMap.get(folder.id) ?? null
                const watchState = watchStateMap.get(folder.id) ?? null
                return (
                  <FolderRow
                    key={folder.id}
                    folder={folder}
                    isIndexing={isIndexing}
                    isRemoving={isRemoving}
                    progress={progress}
                    watchState={watchState}
                    busy={busy}
                    onIndex={() => void handleIndexFolder(folder)}
                    onRemove={() => void handleRemoveFolder(folder)}
                    onToggleWatch={() => void handleToggleWatch(folder)}
                    onToggleFileType={(fileType) => toggleFolderFileType(folder, fileType)}
                  />
                )
              })}
            </div>
          )}
        </section>
      </div>
      <ToastStack toasts={toasts} exitingIds={exitingIds} onDismiss={dismissToast} />
    </AppShell>
  )
}

type FolderRowProps = {
  folder: FolderRecord
  isIndexing: boolean
  isRemoving: boolean
  progress: FolderProgress | null
  watchState: FolderWatchState | null
  busy: boolean
  onIndex: () => void
  onRemove: () => void
  onToggleWatch: () => void
  onToggleFileType: (fileType: string) => void
}

function FolderRow({
  folder,
  isIndexing,
  isRemoving,
  progress,
  watchState,
  busy,
  onIndex,
  onRemove,
  onToggleWatch,
  onToggleFileType,
}: FolderRowProps) {
  const isIndexed = folder.lastIndexedAt !== null
  const watchButtonLabel = folder.watchEnabled ? 'Disable watch' : 'Enable watch'

  let badge: React.ReactNode = isIndexed ? (
    <span className="badge badge-success"><span className="badge-dot" />Ready</span>
  ) : (
    <span className="badge badge-warn"><span className="badge-dot" />Needs index</span>
  )
  if (isIndexing) {
    badge = <span className="badge badge-busy"><span className="badge-dot" />Indexing</span>
  }

  return (
    <article className={`folder-card${isIndexing ? ' is-busy' : ''}`}>
      <div className="folder-card-top">
        <div className="folder-card-title">
          <h3>{folder.displayName}</h3>
          <code className="folder-path" title={folder.path}>{folder.path}</code>
        </div>
        {badge}
      </div>

      <div className="folder-meta">
        <span>{formatIndexedAt(folder.lastIndexedAt)}</span>
        <span>{folder.watchEnabled ? 'Watch enabled' : 'Manual indexing'}</span>
      </div>

      {watchState ? <WatchStatusRow watchState={watchState} /> : null}

      {progress ? <IndexProgressBar progress={progress} /> : null}

      <div className="file-type-grid" role="group" aria-label="File types">
        {SUPPORTED_FILE_TYPES.map((fileType) => {
          const checked = folder.fileTypes.includes(fileType)
          const disabled = busy || (checked && folder.fileTypes.length === 1)
          return (
            <label
              className={`file-type-option${checked ? ' is-selected' : ''}${disabled ? ' is-disabled' : ''}`}
              key={fileType}
            >
              <input
                type="checkbox"
                checked={checked}
                disabled={disabled}
                onChange={() => onToggleFileType(fileType)}
              />
              <CheckGlyph />
              <span>{fileType.toUpperCase()}</span>
            </label>
          )
        })}
      </div>

      <div className="folder-card-actions">
        <button type="button" className="btn btn-sm btn-secondary" disabled={busy} onClick={onIndex}>
          {isIndexing ? 'Indexing…' : isIndexed ? 'Reindex' : 'Run first index'}
        </button>
        <button type="button" className="btn btn-sm btn-ghost" disabled={busy} onClick={onToggleWatch}>
          {watchButtonLabel}
        </button>
        <button type="button" className="btn btn-sm btn-danger" disabled={busy} onClick={onRemove}>
          {isRemoving ? 'Removing…' : 'Remove'}
        </button>
      </div>
    </article>
  )
}

function WatchStatusRow({ watchState }: { watchState: FolderWatchState }) {
  let message = 'Watching for changes…'

  switch (watchState.phase) {
    case 'change-detected':
      message = watchState.path ? `Change detected: ${basename(watchState.path)}` : 'Change detected'
      break
    case 'remove-detected':
      message = watchState.path ? `Removed: ${basename(watchState.path)}` : 'File removed'
      break
    case 'reindex-started':
      message = 'Applying file changes…'
      break
    case 'reindex-completed':
      message = 'Background reindex complete.'
      break
    case 'reindex-failed':
      message = watchState.error ?? 'Background reindex failed.'
      break
    case 'started':
    default:
      message = 'Watching for changes…'
      break
  }

  return <p className={`watch-status${watchState.phase === 'reindex-failed' ? ' is-error' : ''}`}>{message}</p>
}

function IndexProgressBar({ progress }: { progress: FolderProgress }) {
  // Show determinate progress when the backend has reported a total file count;
  // fall back to indeterminate shimmer during discovery (total = 0) and the
  // explicit finalizing phase where percent is not meaningful.
  const indeterminate = progress.finalizing || progress.total === 0
  const percent = indeterminate ? 100 : Math.min(100, Math.round((progress.processed / Math.max(progress.total, 1)) * 100))
  const label = progress.finalizing
    ? 'Finalizing index…'
    : progress.total === 0
      ? 'Scanning…'
      : `${formatCount(progress.processed)} / ${formatCount(progress.total)} files`
  return (
    <div className="progress" role="group" aria-label="Indexing progress">
      <div className={`progress-bar${indeterminate ? ' is-indeterminate' : ''}`}>
        <div
          className="progress-fill"
          style={{ width: `${percent}%` }}
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={indeterminate ? undefined : percent}
        />
      </div>
      <div className="progress-meta">
        <span>{label}</span>
        {progress.currentPath ? (
          <span className="progress-meta-path" title={progress.currentPath}>{basename(progress.currentPath)}</span>
        ) : null}
      </div>
    </div>
  )
}

function CheckGlyph() {
  return (
    <svg className="file-type-check" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="m1.5 5 2.5 2.5L8.5 2.5" />
    </svg>
  )
}
