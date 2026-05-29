import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { AppShell } from './AppShell'
import { ToastStack, useToasts } from './Toast'
import type { FileChunk, FolderRecord, SearchDocumentItem } from './types'
import {
  basename,
  compactPath,
  formatCount,
  formatError,
  formatFolderCount,
  hasTauriInvoke,
  openView,
  statusTone,
} from './utils'

const SEARCH_LIMIT = 20

export default function MainWindow() {
  const [folders, setFolders] = useState<FolderRecord[]>([])
  const [results, setResults] = useState<SearchDocumentItem[]>([])
  const [query, setQuery] = useState('')
  const [searchedQuery, setSearchedQuery] = useState('')
  const [status, setStatus] = useState('Loading library...')
  const [busy, setBusy] = useState(false)
  // File viewer state — null when the dialog is closed.
  const [expandedResult, setExpandedResult] = useState<SearchDocumentItem | null>(null)
  const [expandChunks, setExpandChunks] = useState<FileChunk[]>([])
  const [expandBusy, setExpandBusy] = useState(false)
  const searchInputRef = useRef<HTMLInputElement>(null)
  // Tracks whether we already loaded folders so window-focus refreshes never
  // clobber an in-flight search status with the idle library description.
  const initialLoadDoneRef = useRef(false)
  const { toasts, exitingIds, push: pushToast, dismiss: dismissToast } = useToasts()

  const indexedFolderCount = folders.filter((folder) => folder.lastIndexedAt !== null).length
  const draftQuery = query.trim()
  const hasSearchResults = searchedQuery.length > 0
  const isDraftDirty = draftQuery !== searchedQuery
  // Input stays enabled while searching so users can edit/refine the next query
  // mid-flight; only the submit button is disabled to prevent duplicate work.
  const inputDisabled = !hasTauriInvoke || indexedFolderCount === 0
  const submitDisabled = inputDisabled || busy
  const tone = statusTone(status, busy)

  function focusInput(selectAll = false) {
    searchInputRef.current?.focus()
    if (selectAll) searchInputRef.current?.select()
  }

  function describeIdle(nextFolders: FolderRecord[]) {
    if (!hasTauriInvoke) {
      return 'Browser preview. Search runs in the desktop app.'
    }
    const ready = nextFolders.filter((folder) => folder.lastIndexedAt !== null).length
    const pending = nextFolders.length - ready
    if (nextFolders.length === 0) return 'Add a folder in Library, then run its first index.'
    if (ready === 0) return 'Run the first index in Library to start searching.'
    if (pending === 0) return `Ready across ${formatFolderCount(ready)}.`
    return `Ready across ${formatFolderCount(ready)} · ${formatFolderCount(pending)} pending`
  }

  async function refreshFolders() {
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
        const next = await refreshFolders()
        if (cancelled) return
        setStatus(describeIdle(next))
      } catch (error) {
        if (!cancelled) setStatus(formatError(error))
      } finally {
        initialLoadDoneRef.current = true
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    function handleFocus() {
      void (async () => {
        try {
          const next = await refreshFolders()
          // Don't overwrite a result/status message after the initial load —
          // just refresh the folder list silently in the background.
          if (!initialLoadDoneRef.current || hasSearchResults || busy) return
          setStatus(describeIdle(next))
        } catch {
          // Silent on background refresh failure; manual search will surface errors.
        }
      })()
    }
    window.addEventListener('focus', handleFocus)
    return () => window.removeEventListener('focus', handleFocus)
  }, [hasSearchResults, busy])

  useEffect(() => {
    function handleKey(event: KeyboardEvent) {
      if (event.defaultPrevented) return
      const target = event.target
      const isEditable =
        target instanceof HTMLElement && target.closest('input, textarea, [contenteditable="true"]') !== null
      const isPalette = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k'
      const isSlash = !isEditable && event.key === '/'
      if (isPalette || isSlash) {
        event.preventDefault()
        focusInput(true)
        return
      }
      if (event.key === 'Escape' && document.activeElement === searchInputRef.current) {
        if (query.length > 0) {
          event.preventDefault()
          if (query.trim() === searchedQuery) {
            handleClear()
            return
          }
          setQuery('')
          return
        }
        searchInputRef.current?.blur()
      }
    }
    window.addEventListener('keydown', handleKey)
    return () => window.removeEventListener('keydown', handleKey)
  }, [query, searchedQuery])

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()

    if (!draftQuery) {
      if (hasSearchResults) {
        handleClear()
        return
      }
      setStatus(indexedFolderCount > 0 ? 'Type a topic, phrase, or question.' : describeIdle(folders))
      focusInput()
      return
    }

    if (!hasTauriInvoke) {
      setStatus('Search runs in the desktop app.')
      return
    }

    if (indexedFolderCount === 0) {
      setStatus(
        folders.length === 0
          ? 'Add a folder in Library before searching.'
          : 'Run the first index in Library before searching.',
      )
      return
    }

    setBusy(true)
    setStatus(`Searching for "${draftQuery}"…`)

    try {
      const next = await invoke<SearchDocumentItem[]>('search_document_library', {
        query: draftQuery,
        folderId: null,
        limit: SEARCH_LIMIT,
      })
      setResults(next)
      setSearchedQuery(draftQuery)
      setStatus(
        next.length > 0
          ? `${formatCount(next.length)} result${next.length === 1 ? '' : 's'} for "${draftQuery}"`
          : `No matches for "${draftQuery}"`,
      )
    } catch (error) {
      setStatus(formatError(error))
      pushToast('error', formatError(error))
    } finally {
      setBusy(false)
    }
  }

  function handleClear() {
    setQuery('')
    setSearchedQuery('')
    setResults([])
    setStatus(describeIdle(folders))
    focusInput(true)
  }

  async function handleOpen(path: string) {
    if (!hasTauriInvoke) return
    try {
      await invoke('open_document_path', { path })
    } catch (error) {
      pushToast('error', formatError(error))
    }
  }

  async function handleReveal(path: string) {
    if (!hasTauriInvoke) return
    try {
      await invoke('reveal_document_path', { path })
    } catch (error) {
      pushToast('error', formatError(error))
    }
  }

  async function handleExpand(result: SearchDocumentItem) {
    setExpandedResult(result)
    setExpandChunks([])
    if (!hasTauriInvoke) return
    setExpandBusy(true)
    try {
      const chunks = await invoke<FileChunk[]>('read_document_chunks', {
        absolutePath: result.absolutePath,
      })
      setExpandChunks(chunks)
    } catch (error) {
      pushToast('error', formatError(error))
    } finally {
      setExpandBusy(false)
    }
  }

  function handleCloseExpand() {
    setExpandedResult(null)
    setExpandChunks([])
  }

  return (
    <AppShell currentView="main">
      <div className="window-main">
        <header className="main-header">
          <h1>Search</h1>
          <p className={`main-header-status status-${tone}`} aria-live="polite">
            {(tone === 'busy' || tone === 'error') && <span className="status-dot" aria-hidden="true" />}
            <span>{status}</span>
          </p>
        </header>

        <div className="main-results-wrap">
          {results.length === 0 ? (
            <EmptyState
              hasTauri={hasTauriInvoke}
              folderCount={folders.length}
              indexedCount={indexedFolderCount}
              searchedQuery={hasSearchResults ? searchedQuery : null}
              onClear={handleClear}
              onFocus={() => focusInput()}
            />
          ) : (
            <ul className="results-list">
              {results.map((result, index) => (
                <li key={`${result.absolutePath}:${result.section}:${index}`}>
                  <ResultCard
                    result={result}
                    query={searchedQuery}
                    onOpen={() => void handleOpen(result.absolutePath)}
                    onReveal={() => void handleReveal(result.absolutePath)}
                    onExpand={() => void handleExpand(result)}
                  />
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="main-composer-wrap">
          <form className="composer" onSubmit={handleSubmit} role="search">
            <SearchGlyph />
            <input
              ref={searchInputRef}
              type="text"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              disabled={inputDisabled}
              spellCheck={false}
              aria-label="Search"
              placeholder={
                !hasTauriInvoke
                  ? 'Search runs in the desktop app'
                  : folders.length === 0
                    ? 'Add a folder in Library to start searching'
                    : indexedFolderCount === 0
                      ? 'Run the first index in Library to start searching'
                      : 'Search by topic, file name, or question'
              }
            />
            <div className="composer-actions">
              {hasSearchResults && !isDraftDirty ? (
                // Distinct clear control prevents the ambiguous "submit becomes
                // clear" affordance the old composer had.
                <button type="button" className="composer-clear" onClick={handleClear} aria-label="Clear search">
                  <CloseGlyph />
                </button>
              ) : null}
              <button
                type="submit"
                className="composer-submit"
                disabled={submitDisabled}
                aria-label={busy ? 'Searching' : 'Search'}
              >
                <SubmitGlyph />
              </button>
            </div>
          </form>

          <div className="composer-hint">
            <span className="composer-hint-left">
              {busy ? 'Searching…' : isDraftDirty && draftQuery ? 'Press Enter to search.' : <>Press <kbd>/</kbd> to focus search.</>}
            </span>
            <span className="composer-hint-right">
              {indexedFolderCount > 0 && `${formatFolderCount(indexedFolderCount)} indexed`}
            </span>
          </div>
        </div>
      </div>
      <ToastStack toasts={toasts} exitingIds={exitingIds} onDismiss={dismissToast} />
      {expandedResult !== null && (
        <FileViewerDialog
          result={expandedResult}
          chunks={expandChunks}
          busy={expandBusy}
          onClose={handleCloseExpand}
        />
      )}
    </AppShell>
  )
}

type ResultCardProps = {
  result: SearchDocumentItem
  query: string
  onOpen: () => void
  onReveal: () => void
  onExpand: () => void
}

function ResultCard({ result, query, onOpen, onReveal, onExpand }: ResultCardProps) {
  const meta = [
    result.folderName,
    result.section !== 'n/a' && result.section !== '(root)' ? result.section : null,
    result.page !== 'n/a' ? `Page ${result.page}` : null,
  ].filter((value): value is string => value !== null)

  const location = result.filePath.trim() || compactPath(result.absolutePath)

  return (
    <article className="result-card">
      <div className="result-card-leading" aria-hidden="true">
        <FileGlyph path={result.absolutePath} />
      </div>
      <div className="result-card-body">
        <div className="result-title-row">
          <span className="result-title" title={result.title}>{result.title}</span>
          <span className="result-score" aria-label={`Match score ${formatScore(result.score)}`}>
            <span className="result-score-dot" aria-hidden="true" />
            {formatScore(result.score)}
          </span>
        </div>
        <div className="result-meta">
          {meta.map((item) => (
            <span key={item}>{item}</span>
          ))}
        </div>
        <p className="result-preview">
          <Highlight text={result.preview} query={query} />
        </p>
        <p className="result-path" title={result.absolutePath}>{location}</p>
      </div>
      <div className="result-actions">
        <button type="button" className="btn btn-sm btn-secondary" onClick={onOpen}>
          Open
        </button>
        <button type="button" className="btn btn-sm btn-ghost" onClick={onReveal}>
          Reveal
        </button>
        <button type="button" className="btn btn-sm btn-ghost" onClick={onExpand}>
          Expand
        </button>
      </div>
    </article>
  )
}

type EmptyStateProps = {
  hasTauri: boolean
  folderCount: number
  indexedCount: number
  searchedQuery: string | null
  onClear: () => void
  onFocus: () => void
}

function EmptyState({ hasTauri, folderCount, indexedCount, searchedQuery, onClear, onFocus }: EmptyStateProps) {
  let title = 'Start with a topic, phrase, or question'
  let body = `Search across ${formatFolderCount(indexedCount)} by title, section, exact phrase, or natural-language question.`
  let hint = 'Press / or ⌘K to focus the search field.'
  let primary = (
    <button type="button" className="btn btn-primary" onClick={onFocus}>
      Focus search
    </button>
  )
  let secondary: React.ReactNode = null

  if (!hasTauri) {
    title = 'Search runs in the desktop app'
    body = 'This browser preview shows the layout. Add and index folders in the desktop app to search them.'
    hint = ''
    primary = (
      <button type="button" className="btn btn-secondary" onClick={() => void openView('library', 'main')}>
        Open Library
      </button>
    )
  } else if (folderCount === 0) {
    title = 'Your library is empty'
    body = 'Add a local folder in Library, then run its first index to make it searchable.'
    hint = 'The source files stay where they are. Compas builds a separate search index.'
    primary = (
      <button type="button" className="btn btn-primary" onClick={() => void openView('library', 'main')}>
        Open Library
      </button>
    )
  } else if (indexedCount === 0) {
    title = 'Index a folder to start searching'
    body = `${formatFolderCount(folderCount)} added, none indexed yet.`
    hint = 'Run the first index in Library. Search becomes available immediately after.'
    primary = (
      <button type="button" className="btn btn-primary" onClick={() => void openView('library', 'main')}>
        Open Library
      </button>
    )
  } else if (searchedQuery !== null) {
    title = `No matches for "${searchedQuery}"`
    body = 'Try a broader topic, a shorter phrase, or a document name that should contain the answer.'
    hint = 'If you recently added files, verify the folder has been indexed in Library.'
    primary = (
      <button type="button" className="btn btn-primary" onClick={onClear}>
        Clear search
      </button>
    )
    secondary = (
      <button type="button" className="btn btn-ghost" onClick={() => void openView('library', 'main')}>
        Check Library
      </button>
    )
  }

  return (
    <div className="empty-state">
      <div className="empty-state-icon" aria-hidden="true">
        <SearchGlyph />
      </div>
      <h3>{title}</h3>
      <p>{body}</p>
      {hint ? <p className="empty-state-hint">{hint}</p> : null}
      <div className="empty-state-actions">
        {primary}
        {secondary}
      </div>
    </div>
  )
}

type FileViewerDialogProps = {
  result: SearchDocumentItem
  chunks: FileChunk[]
  busy: boolean
  onClose: () => void
}

// Renders all indexed chunks for a file in a scrollable modal and auto-scrolls
// to the chunk that matches the search result's preview text.
function FileViewerDialog({ result, chunks, busy, onClose }: FileViewerDialogProps) {
  // Find the chunk whose preview matches the search result — they share the same
  // value because both come from the same ChunkRow stored in SQLite.
  const activeIndex = chunks.findIndex((chunk) => chunk.preview === result.preview)
  const activeRef = useRef<HTMLDivElement>(null)

  // Scroll to the active chunk once the chunks are rendered.
  useEffect(() => {
    if (activeRef.current) {
      activeRef.current.scrollIntoView({ block: 'center', behavior: 'smooth' })
    }
  }, [chunks])

  // Close on Escape key.
  useEffect(() => {
    function handleKey(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        event.preventDefault()
        onClose()
      }
    }
    window.addEventListener('keydown', handleKey)
    return () => window.removeEventListener('keydown', handleKey)
  }, [onClose])

  const section =
    result.section !== 'n/a' && result.section !== '(root)' ? result.section : null
  const page = result.page !== 'n/a' ? `Page ${result.page}` : null

  return (
    // Backdrop — click outside dialog to dismiss.
    <div className="file-viewer-backdrop" onClick={onClose} role="presentation">
      <div
        className="file-viewer-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={`File contents: ${result.title}`}
        // Stop clicks inside the dialog from bubbling to the backdrop.
        onClick={(event) => event.stopPropagation()}
      >
        <div className="file-viewer-header">
          <div className="file-viewer-title-wrap">
            <span className="file-viewer-icon" aria-hidden="true">
              <FileGlyph path={result.absolutePath} />
            </span>
            <div className="file-viewer-title-block">
              <span className="file-viewer-title" title={result.title}>{result.title}</span>
              <span className="file-viewer-subtitle">
                {[result.folderName, section, page].filter(Boolean).join(' · ')}
              </span>
            </div>
          </div>
          <button
            type="button"
            className="btn btn-icon btn-ghost file-viewer-close"
            onClick={onClose}
            aria-label="Close"
          >
            <CloseGlyph />
          </button>
        </div>

        <div className="file-viewer-body">
          {busy && chunks.length === 0 ? (
            <div className="file-viewer-loading">
              <span className="status-dot" aria-hidden="true" />
              Loading…
            </div>
          ) : chunks.length === 0 ? (
            <div className="file-viewer-empty">
              No indexed content for this file. Re-index the folder to view its contents.
            </div>
          ) : (
            (() => {
              // Compute which indices should show a heading: only when it
              // differs from the previous chunk's heading, so a long section
              // with many chunks shows its heading exactly once.
              const showHeadingAt = new Set<number>()
              let lastHeading = ''
              chunks.forEach((chunk, index) => {
                const heading = chunk.headingPath.join(' › ')
                if (heading !== lastHeading) {
                  showHeadingAt.add(index)
                  lastHeading = heading
                }
              })

              return chunks.map((chunk, index) => {
                const isActive = index === activeIndex
                const heading =
                  chunk.headingPath.length > 0 ? chunk.headingPath.join(' › ') : null
                const pageLabel =
                  chunk.pageStart !== null
                    ? chunk.pageEnd !== null && chunk.pageEnd !== chunk.pageStart
                      ? `Pages ${chunk.pageStart}–${chunk.pageEnd}`
                      : `Page ${chunk.pageStart}`
                    : null

                // Show the heading only when it changes; page labels still
                // show per-chunk since each chunk may span different pages.
                const showHeading = heading !== null && showHeadingAt.has(index)

                return (
                  <div
                    key={chunk.chunkId}
                    ref={isActive ? activeRef : undefined}
                    className={`file-chunk${isActive ? ' is-active' : ''}`}
                    aria-current={isActive ? 'true' : undefined}
                  >
                    {(showHeading || pageLabel !== null) && (
                      <div className="file-chunk-meta">
                        {showHeading && <span className="file-chunk-heading">{heading}</span>}
                        {pageLabel && <span className="file-chunk-page">{pageLabel}</span>}
                      </div>
                    )}
                    <p className="file-chunk-text">{chunk.text}</p>
                  </div>
                )
              })
            })()
          )}
        </div>
      </div>
    </div>
  )
}

function formatScore(score: number) {
  const normalized = Math.min(Math.max(score, 0), 1)
  return `${(normalized * 100).toFixed(0)}%`
}

// Small inline file glyph keyed on extension — replaces react-file-icon
// to keep the bundle lighter and the visual language consistent with the
// rest of the icon set.
function FileGlyph({ path }: { path: string }) {
  const ext = (path.split('.').pop() ?? '').toLowerCase()
  const label = labelForExtension(ext)
  const tone = toneForExtension(ext)
  return (
    <svg viewBox="0 0 36 44" role="img" aria-label={`${label || basename(path)} file`}>
      <path
        d="M5 2h18l8 8v30a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2Z"
        fill="var(--surface)"
        stroke="var(--border-strong)"
      />
      <path d="M23 2v8h8" fill="var(--surface-2)" stroke="var(--border-strong)" />
      {label ? (
        <text
          x="18"
          y="32"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="7.5"
          fontWeight="700"
          fill={tone}
        >
          {label}
        </text>
      ) : null}
    </svg>
  )
}

function labelForExtension(ext: string): string {
  switch (ext) {
    case 'pdf':
      return 'PDF'
    case 'md':
    case 'markdown':
      return 'MD'
    case 'txt':
      return 'TXT'
    case 'doc':
    case 'docx':
      return 'DOC'
    case 'xls':
    case 'xlsx':
      return 'XLS'
    default:
      return ext ? ext.toUpperCase().slice(0, 4) : ''
  }
}

function toneForExtension(ext: string): string {
  switch (ext) {
    case 'pdf':
      return 'var(--danger)'
    case 'md':
    case 'markdown':
      return 'var(--accent)'
    case 'txt':
      return 'var(--text-muted)'
    case 'doc':
    case 'docx':
      return 'var(--accent-strong)'
    case 'xls':
    case 'xlsx':
      return 'var(--success)'
    default:
      return 'var(--text-subtle)'
  }
}

function Highlight({ text, query }: { text: string; query: string }) {
  const tokens = Array.from(
    new Set(
      query
        .toLowerCase()
        .split(/[^a-z0-9]+/i)
        .map((token) => token.trim())
        .filter((token) => token.length > 1),
    ),
  )
  if (tokens.length === 0) return <>{text}</>
  const pattern = new RegExp(`(${tokens.map(escapeRegExp).join('|')})`, 'gi')
  const parts = text.split(pattern)
  return (
    <>
      {parts.map((part, index) =>
        tokens.some((token) => token.toLowerCase() === part.toLowerCase()) ? (
          <mark key={index}>{part}</mark>
        ) : (
          <span key={index}>{part}</span>
        ),
      )}
    </>
  )
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function SearchGlyph() {
  return (
    <svg className="composer-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <circle cx="7" cy="7" r="4.5" />
      <path d="m10.5 10.5 3 3" />
    </svg>
  )
}

function SubmitGlyph() {
  return (
    <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 10h10" />
      <path d="m10.5 5 5 5-5 5" />
    </svg>
  )
}

function CloseGlyph() {
  return (
    <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <path d="m3 3 8 8M11 3l-8 8" />
    </svg>
  )
}
