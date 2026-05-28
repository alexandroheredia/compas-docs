import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'
import type { FormEvent, ReactNode } from 'react'
import { FileIcon, defaultStyles } from 'react-file-icon'
import { AppShell } from './AppShell'
import type { FolderRecord, SearchDocumentItem } from './types'
import {
  compactPath,
  formatCount,
  formatError,
  formatFolderCount,
  hasTauriInvoke,
  openView,
  statusTone,
} from './utils'

export default function MainWindow() {
  const [folders, setFolders] = useState<FolderRecord[]>([])
  const [results, setResults] = useState<SearchDocumentItem[]>([])
  const [query, setQuery] = useState('')
  const [searchedQuery, setSearchedQuery] = useState('')
  const [status, setStatus] = useState('Loading your library...')
  const [busy, setBusy] = useState(false)
  const searchInputRef = useRef<HTMLInputElement>(null)

  const indexedFolderCount = folders.filter((folder) => folder.lastIndexedAt !== null).length
  const draftQuery = query.trim()
  const hasSearchResults = searchedQuery.length > 0
  const isSearchDraftDirty = draftQuery !== searchedQuery
  const searchDisabled = !hasTauriInvoke || indexedFolderCount === 0 || busy
  const tone = statusTone(status, busy)

  function pushStatus(message: string) {
    setStatus(message)
  }

  function focusSearchInput(selectText = false) {
    searchInputRef.current?.focus()

    if (selectText) {
      searchInputRef.current?.select()
    }
  }

  function describeIdleState(nextFolders: FolderRecord[]) {
    if (!hasTauriInvoke) {
      return 'Browser preview only. Search runs in the desktop app.'
    }

    const nextIndexedFolderCount = nextFolders.filter((folder) => folder.lastIndexedAt !== null).length
    const pendingFolderCount = nextFolders.length - nextIndexedFolderCount

    if (nextFolders.length === 0) {
      return 'Add a folder in Library, then run its first index to start searching.'
    }

    if (nextIndexedFolderCount === 0) {
      return 'Your library has folders, but none has been indexed yet.'
    }

    if (pendingFolderCount === 0) {
      return `Ready to search across ${formatFolderCount(nextIndexedFolderCount)}.`
    }

    return `Ready to search across ${formatFolderCount(nextIndexedFolderCount)}. ${formatFolderCount(pendingFolderCount)} still ${pendingFolderCount === 1 ? 'needs' : 'need'} a first index.`
  }

  async function refreshLibrary() {
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
        const nextFolders = await refreshLibrary()
        if (cancelled) {
          return
        }

        pushStatus(describeIdleState(nextFolders))
      } catch (error) {
        if (!cancelled) {
          pushStatus(formatError(error))
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
          const nextFolders = await refreshLibrary()
          pushStatus(describeIdleState(nextFolders))
        } catch (error) {
          pushStatus(formatError(error))
        }
      })()
    }

    window.addEventListener('focus', handleWindowFocus)

    return () => {
      window.removeEventListener('focus', handleWindowFocus)
    }
  }, [])

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented) {
        return
      }

      const target = event.target
      const isEditable =
        target instanceof HTMLElement && target.closest('input, textarea, [contenteditable="true"]') !== null
      const pressedPaletteShortcut = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k'
      const pressedSlashShortcut = !isEditable && event.key === '/'

      if (pressedSlashShortcut || pressedPaletteShortcut) {
        event.preventDefault()
        focusSearchInput(true)
        return
      }

      if (event.key === 'Escape' && document.activeElement === searchInputRef.current) {
        event.preventDefault()

        if (query.length > 0) {
          if (query.trim() === searchedQuery) {
            handleClearSearch()
            return
          }

          setQuery('')
          return
        }

        searchInputRef.current?.blur()
      }
    }

    window.addEventListener('keydown', handleKeyDown)

    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [query, searchedQuery, folders])

  async function handleSearch(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()

    if (!draftQuery) {
      if (hasSearchResults) {
        handleClearSearch()
        return
      }

      pushStatus(indexedFolderCount > 0 ? 'Type a topic, phrase, or question first.' : describeIdleState(folders))
      focusSearchInput()
      return
    }

    if (!hasTauriInvoke) {
      pushStatus('Search runs in the desktop app.')
      focusSearchInput(true)
      return
    }

    if (indexedFolderCount === 0) {
      pushStatus(
        folders.length === 0
          ? 'Add a folder in Library before searching.'
          : 'Run the first index in Library before searching.',
      )
      return
    }

    setBusy(true)
    pushStatus(`Searching for "${draftQuery}"...`)

    try {
      const nextResults = await invoke<SearchDocumentItem[]>('search_document_library', {
        query: draftQuery,
        folderId: null,
        limit: 20,
      })

      setResults(nextResults)
      setSearchedQuery(draftQuery)
      pushStatus(
        nextResults.length > 0
          ? `Showing ${formatCount(nextResults.length)} result${nextResults.length === 1 ? '' : 's'} for "${draftQuery}".`
          : `No matches for "${draftQuery}".`,
      )
    } catch (error) {
      pushStatus(formatError(error))
    } finally {
      setBusy(false)
    }
  }

  function handleClearSearch() {
    setQuery('')
    setSearchedQuery('')
    setResults([])
    pushStatus(describeIdleState(folders))
    focusSearchInput(true)
  }

  async function handleOpenLibrary() {
    try {
      await openView('library', 'main')
    } catch (error) {
      pushStatus(formatError(error))
    }
  }

  async function handleOpen(path: string) {
    if (!hasTauriInvoke) {
      return
    }

    try {
      await invoke('open_document_path', { path })
      pushStatus(`Opened ${compactPath(path)}.`)
    } catch (error) {
      pushStatus(formatError(error))
    }
  }

  async function handleReveal(path: string) {
    if (!hasTauriInvoke) {
      return
    }

    try {
      await invoke('reveal_document_path', { path })
      pushStatus(`Revealed ${compactPath(path)} in Finder.`)
    } catch (error) {
      pushStatus(formatError(error))
    }
  }

  const searchContextItems = [
    'Semantic search',
    !hasTauriInvoke
      ? 'Preview mode'
      : indexedFolderCount === 0
        ? folders.length === 0
          ? 'No folders yet'
          : 'Needs first index'
        : `${formatFolderCount(indexedFolderCount)} indexed`,
    'Top 20 results',
  ]

  const resultSummary =
    tone === 'error'
      ? status
      : busy
        ? `Searching for "${draftQuery || query}"...`
        : hasSearchResults
          ? results.length > 0
            ? `Showing ${formatCount(results.length)} result${results.length === 1 ? '' : 's'} for "${searchedQuery}"`
            : `No matches for "${searchedQuery}"`
          : status

  let emptyStateTitle = 'Start with a topic, phrase, or question'
  let emptyStateCopy = `Search across ${formatFolderCount(indexedFolderCount)} by document title, section heading, exact phrase, or natural-language question.`
  let emptyStateHint = 'Press / or Cmd/Ctrl+K to focus search, then press Enter to update the results.'
  let primaryEmptyAction: ReactNode = (
    <button type="button" className="primary-button" onClick={() => focusSearchInput()}>
      Focus search
    </button>
  )
  let secondaryEmptyAction: ReactNode = null

  if (!hasTauriInvoke) {
    emptyStateTitle = 'Search runs in the desktop app'
    emptyStateCopy = 'This browser preview shows the layout. Add and index folders in the desktop app to search them.'
    emptyStateHint = 'Open Library, add a local folder, run its first index, then search by topic, phrase, or question.'
    primaryEmptyAction = (
      <button type="button" className="primary-button" onClick={() => void handleOpenLibrary()}>
        Open Library
      </button>
    )
  } else if (folders.length === 0) {
    emptyStateTitle = 'Your library is empty'
    emptyStateCopy = 'Add a local folder in Library, then run its first index to make it searchable.'
    emptyStateHint = 'Compas Docs keeps the source folder in place and stores its search index separately.'
    primaryEmptyAction = (
      <button type="button" className="primary-button" onClick={() => void handleOpenLibrary()}>
        Open Library
      </button>
    )
  } else if (indexedFolderCount === 0) {
    emptyStateTitle = 'Index a folder to start searching'
    emptyStateCopy = `${formatFolderCount(folders.length)} ${folders.length === 1 ? 'is' : 'are'} in the library, but none has been indexed yet.`
    emptyStateHint = 'Run the first index in Library. Search becomes available as soon as at least one folder is ready.'
    primaryEmptyAction = (
      <button type="button" className="primary-button" onClick={() => void handleOpenLibrary()}>
        Open Library
      </button>
    )
  } else if (hasSearchResults) {
    emptyStateTitle = `No matches for "${searchedQuery}"`
    emptyStateCopy = 'Try a broader topic, a shorter phrase, or a document name that should contain the answer.'
    emptyStateHint = 'If you recently added files, make sure the folder has been indexed in Library.'
    primaryEmptyAction = (
      <button type="button" className="primary-button" onClick={handleClearSearch}>
        Clear search
      </button>
    )
    secondaryEmptyAction = (
      <button type="button" className="secondary-button" onClick={() => void handleOpenLibrary()}>
        Check Library
      </button>
    )
  }

  return (
    <AppShell currentView="main">
      <section className="window-layout window-layout-main">
        <header className="window-toolbar window-toolbar-search">
          <div className="toolbar-main">
            <div className="toolbar-context" aria-label="Search context">
              <span className="section-caption">Search context</span>
              {searchContextItems.map((item) => (
                <span className="toolbar-chip" key={item}>
                  {item}
                </span>
              ))}
            </div>
            <h2>Results</h2>
          </div>

          <p className={`toolbar-copy toolbar-copy-${tone}`} aria-live="polite">
            {resultSummary}
          </p>
        </header>

        <section className="results-scroll">
          {results.length === 0 ? (
            <div className="empty-state empty-state-large empty-state-main">
              <h3>{emptyStateTitle}</h3>
              <p>{emptyStateCopy}</p>
              <p className="empty-state-hint">{emptyStateHint}</p>
              <div className="empty-state-actions">
                {primaryEmptyAction}
                {secondaryEmptyAction}
              </div>
            </div>
          ) : (
            <div className="result-stack">
              {results.map((result, index) => {
                const fileType = getFileTypeInfo(result.filePath)
                const resultMeta = buildResultMeta(result)
                const resultLocation = result.filePath.trim() || compactPath(result.absolutePath)

                return (
                  <article className="result-card result-card-rich" key={`${result.absolutePath}:${result.section}:${index}`}>
                    <div className="result-card-leading">
                      <div className={`file-icon-wrap file-icon-wrap-${fileType.tone}`} title={`${fileType.label} document`}>
                        <FileIcon extension={fileType.extension} {...fileType.styles} />
                      </div>
                    </div>

                    <div className="result-card-body">
                      <div className="result-card-title-row">
                        <h3>{result.title}</h3>
                        <div className="result-score-chip" aria-label={`Match score ${formatScore(result.score)}`}>
                          <strong>{formatScore(result.score)}</strong>
                          <span>match</span>
                        </div>
                      </div>

                      <div className="result-meta-row">
                        {resultMeta.map((item) => (
                          <span key={`${result.absolutePath}:${item}`}>{item}</span>
                        ))}
                      </div>

                      <p className="result-preview">
                        <HighlightedText text={result.preview} query={searchedQuery} />
                      </p>

                      <div className="result-footer">
                        <div className="result-location-block">
                          <code className="result-route" title={result.absolutePath}>
                            {resultLocation}
                          </code>
                        </div>

                        <div className="result-actions">
                          <button type="button" className="result-link result-link-primary" onClick={() => void handleOpen(result.absolutePath)}>
                            Open
                          </button>
                          <button type="button" className="result-link" onClick={() => void handleReveal(result.absolutePath)}>
                            Reveal
                          </button>
                        </div>
                      </div>

                      <div className="result-actions-mobile">
                        <button type="button" className="result-link result-link-primary" onClick={() => void handleOpen(result.absolutePath)}>
                          Open
                        </button>
                        <button type="button" className="result-link" onClick={() => void handleReveal(result.absolutePath)}>
                          Reveal
                        </button>
                      </div>
                    </div>
                  </article>
                )
              })}
            </div>
          )}
        </section>

        <form className="search-composer" onSubmit={handleSearch}>
          <div className="composer-card composer-card-rich">
            <div className="composer-top-row">
              <div className="composer-pills" aria-hidden="true">
                <span className="composer-pill composer-pill-primary">Semantic search</span>
                <span className="composer-pill">
                  {!hasTauriInvoke
                    ? 'Preview mode'
                    : indexedFolderCount === 0
                      ? folders.length === 0
                        ? 'No indexed folders'
                        : 'Waiting for first index'
                      : `${formatFolderCount(indexedFolderCount)} indexed`}
                </span>
              </div>

              <span className="composer-caption">
                {busy
                  ? `Searching "${draftQuery || query}"...`
                  : !hasTauriInvoke
                    ? 'Browser preview only.'
                    : folders.length === 0
                      ? 'Add a folder in Library.'
                      : indexedFolderCount === 0
                        ? 'Run the first index in Library.'
                        : hasSearchResults && draftQuery.length === 0
                          ? 'Press Enter to clear current results.'
                          : isSearchDraftDirty
                            ? 'Press Enter to update results.'
                            : <>
                                Press <kbd>/</kbd> to focus search.
                              </>}
              </span>
            </div>

            <div className="composer-row composer-row-rich">
              <input
                id="query-input"
                aria-label="Search"
                ref={searchInputRef}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                disabled={searchDisabled}
                spellCheck={false}
                placeholder={
                  !hasTauriInvoke
                    ? 'Search is available in the desktop app'
                    : folders.length === 0
                      ? 'Add a folder in Library to start searching'
                      : indexedFolderCount === 0
                        ? 'Run the first index in Library to start searching'
                        : 'Search by topic, file name, or question'
                }
              />
              <button
                type="submit"
                className="composer-submit-button"
                disabled={searchDisabled}
                aria-label={busy ? 'Searching' : 'Search'}
              >
                <ComposerSubmitIcon />
              </button>
            </div>
          </div>
        </form>
      </section>
    </AppShell>
  )
}

function formatScore(score: number) {
  const normalizedScore = Math.min(Math.max(score, 0), 1)
  return `${(normalizedScore * 100).toFixed(1)}%`
}

function buildResultMeta(result: SearchDocumentItem) {
  return [
    result.folderName,
    result.section !== 'n/a' && result.section !== '(root)' ? result.section : null,
    result.page !== 'n/a' ? `Page ${result.page}` : null,
  ].filter((value): value is string => value !== null)
}

function getFileTypeInfo(path: string) {
  const extension = path.split('.').pop()?.toLowerCase()

  switch (extension) {
    case 'pdf':
      return {
        label: 'PDF',
        extension: 'pdf',
        tone: 'pdf' as const,
        styles: defaultStyles.pdf,
      }
    case 'doc':
    case 'docx':
      return {
        label: 'DOCX',
        extension: 'docx',
        tone: 'doc' as const,
        styles: defaultStyles.docx,
      }
    case 'xls':
    case 'xlsx':
      return {
        label: 'XLSX',
        extension: 'xlsx',
        tone: 'sheet' as const,
        styles: defaultStyles.xlsx,
      }
    case 'md':
      return {
        label: 'MD',
        extension: 'md',
        tone: 'markdown' as const,
        styles: defaultStyles.md,
      }
    case 'txt':
      return {
        label: 'TXT',
        extension: 'txt',
        tone: 'text' as const,
        styles: defaultStyles.txt,
      }
    default:
      return {
        label: 'FILE',
        extension: 'file',
        tone: 'file' as const,
        styles: defaultStyles.document,
      }
  }
}

function HighlightedText({ text, query }: { text: string; query: string }) {
  const tokens = Array.from(
    new Set(
      query
        .toLowerCase()
        .split(/[^a-z0-9]+/i)
        .map((token) => token.trim())
        .filter((token) => token.length > 1),
    ),
  )

  if (tokens.length === 0) {
    return text
  }

  const pattern = new RegExp(`(${tokens.map(escapeRegExp).join('|')})`, 'gi')
  const parts = text.split(pattern)

  return parts.map((part, index) =>
    tokens.some((token) => token.toLowerCase() === part.toLowerCase()) ? (
      <mark key={`${part}-${index}`}>{part}</mark>
    ) : (
      <span key={`${part}-${index}`}>{part}</span>
    ),
  )
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function ComposerSubmitIcon() {
  return (
    <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M4 10h10" strokeLinecap="round" />
      <path d="m10.5 5 5 5-5 5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}
