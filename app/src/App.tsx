import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import './App.css'

type FolderRecord = {
  id: string
  path: string
  displayName: string
  storagePath: string
  lastIndexedAt: number | null
  watchEnabled: boolean
}

type SearchDocumentItem = {
  folderId: string
  folderName: string
  filePath: string
  absolutePath: string
  title: string
  section: string
  page: string
  preview: string
  score: number
}

type RemoveFolderResponse = {
  removed: boolean
}

function App() {
  const [folders, setFolders] = useState<FolderRecord[]>([])
  const [results, setResults] = useState<SearchDocumentItem[]>([])
  const [folderPath, setFolderPath] = useState('')
  const [query, setQuery] = useState('')
  const [selectedFolderId, setSelectedFolderId] = useState('')
  const [status, setStatus] = useState('Loading folders...')
  const [busy, setBusy] = useState(false)

  async function loadFolders() {
    const nextFolders = await invoke<FolderRecord[]>('list_document_folders')
    setFolders(nextFolders)
    setSelectedFolderId((current) => {
      if (!current) {
        return ''
      }
      return nextFolders.some((folder) => folder.id === current) ? current : ''
    })
    return nextFolders
  }

  useEffect(() => {
    let cancelled = false

    async function run() {
      try {
        const nextFolders = await loadFolders()
        if (cancelled) {
          return
        }
        setStatus(
          nextFolders.length > 0
            ? 'Ready to search your indexed folders.'
            : 'Add a folder and index it to start searching.',
        )
      } catch (error) {
        if (!cancelled) {
          setStatus(formatError(error))
        }
      }
    }

    run()

    return () => {
      cancelled = true
    }
  }, [])

  async function handleAddFolder(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const trimmedPath = folderPath.trim()
    if (!trimmedPath) {
      setStatus('Enter a folder path first.')
      return
    }

    setBusy(true)
    try {
      const record = await invoke<FolderRecord>('add_document_folder', { path: trimmedPath })
      setFolderPath('')
      await loadFolders()
      setSelectedFolderId(record.id)
      setStatus(`Added ${record.displayName}.`)
    } catch (error) {
      setStatus(formatError(error))
    } finally {
      setBusy(false)
    }
  }

  async function handleIndexFolder(folder: FolderRecord) {
    setBusy(true)
    setStatus(`Indexing ${folder.displayName}...`)
    try {
      await invoke<FolderRecord>('index_document_folder', { path: folder.path })
      await loadFolders()
      setStatus(`Indexed ${folder.displayName}.`)
    } catch (error) {
      setStatus(formatError(error))
    } finally {
      setBusy(false)
    }
  }

  async function handleRemoveFolder(folder: FolderRecord) {
    setBusy(true)
    try {
      const response = await invoke<RemoveFolderResponse>('remove_document_folder', {
        id: folder.id,
      })
      await loadFolders()
      if (selectedFolderId === folder.id) {
        setSelectedFolderId('')
      }
      setResults((current) => current.filter((result) => result.folderId !== folder.id))
      setStatus(
        response.removed
          ? `Removed ${folder.displayName}.`
          : `${folder.displayName} was already gone.`,
      )
    } catch (error) {
      setStatus(formatError(error))
    } finally {
      setBusy(false)
    }
  }

  async function handleSearch(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const trimmedQuery = query.trim()
    if (!trimmedQuery) {
      setStatus('Enter a search query first.')
      return
    }

    setBusy(true)
    setStatus('Searching documents...')
    try {
      const nextResults = await invoke<SearchDocumentItem[]>('search_document_library', {
        query: trimmedQuery,
        folderId: selectedFolderId || null,
        limit: 20,
      })
      setResults(nextResults)
      setStatus(
        nextResults.length > 0
          ? `Found ${nextResults.length} result${nextResults.length === 1 ? '' : 's'}.`
          : 'No matches found.',
      )
    } catch (error) {
      setStatus(formatError(error))
    } finally {
      setBusy(false)
    }
  }

  async function handleOpen(path: string) {
    try {
      await invoke('open_document_path', { path })
    } catch (error) {
      setStatus(formatError(error))
    }
  }

  async function handleReveal(path: string) {
    try {
      await invoke('reveal_document_path', { path })
    } catch (error) {
      setStatus(formatError(error))
    }
  }

  return (
    <main className="app-shell">
      <header className="hero">
        <div>
          <p className="eyebrow">Compas Docs</p>
          <h1>Local document search, now with a desktop shell.</h1>
          <p className="hero-copy">
            Add folders, index them into the shared library, then search across your docs without
            leaving the app.
          </p>
        </div>
        <p className="status" aria-live="polite">
          {status}
        </p>
      </header>

      <section className="panel-grid">
        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">Library</p>
              <h2>Folders</h2>
            </div>
            <span className="chip">{folders.length}</span>
          </div>

          <form className="stack" onSubmit={handleAddFolder}>
            <label className="stack">
              <span>Folder path</span>
              <input
                value={folderPath}
                onChange={(event) => setFolderPath(event.target.value)}
                placeholder="/Users/alexandro/Documents/Research"
              />
            </label>
            <button type="submit" disabled={busy}>
              Add folder
            </button>
          </form>

          <div className="folder-list">
            {folders.length === 0 ? (
              <p className="empty-state">No folders yet.</p>
            ) : (
              folders.map((folder) => (
                <article className="folder-card" key={folder.id}>
                  <div className="folder-copy">
                    <strong>{folder.displayName}</strong>
                    <code>{folder.path}</code>
                    <span>
                      {folder.lastIndexedAt
                        ? `Indexed ${new Date(folder.lastIndexedAt * 1000).toLocaleString()}`
                        : 'Not indexed yet'}
                    </span>
                  </div>
                  <div className="row-actions">
                    <button type="button" disabled={busy} onClick={() => void handleIndexFolder(folder)}>
                      Index
                    </button>
                    <button
                      type="button"
                      className="secondary"
                      disabled={busy}
                      onClick={() => setSelectedFolderId(folder.id)}
                    >
                      Search only this
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      disabled={busy}
                      onClick={() => void handleRemoveFolder(folder)}
                    >
                      Remove
                    </button>
                  </div>
                </article>
              ))
            )}
          </div>
        </section>

        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">Search</p>
              <h2>Document library</h2>
            </div>
            <span className="chip">{results.length}</span>
          </div>

          <form className="search-form" onSubmit={handleSearch}>
            <label className="stack search-input">
              <span>Query</span>
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="renewal date"
              />
            </label>
            <label className="stack scope-select">
              <span>Scope</span>
              <select
                value={selectedFolderId}
                onChange={(event) => setSelectedFolderId(event.target.value)}
              >
                <option value="">All folders</option>
                {folders.map((folder) => (
                  <option key={folder.id} value={folder.id}>
                    {folder.displayName}
                  </option>
                ))}
              </select>
            </label>
            <button type="submit" disabled={busy}>
              Search
            </button>
          </form>

          <div className="result-list">
            {results.length === 0 ? (
              <p className="empty-state">Run a search to see matching documents.</p>
            ) : (
              results.map((result) => (
                <article className="result-card" key={`${result.absolutePath}:${result.section}`}>
                  <div className="result-header">
                    <div>
                      <p className="result-title">{result.title}</p>
                      <p className="result-meta">
                        {result.folderName} / {result.filePath}
                      </p>
                    </div>
                    <span className="score">{result.score.toFixed(3)}</span>
                  </div>
                  <p className="result-meta">
                    Section: {result.section} | Page: {result.page}
                  </p>
                  <p className="preview">{result.preview}</p>
                  <div className="row-actions">
                    <button type="button" onClick={() => void handleOpen(result.absolutePath)}>
                      Open
                    </button>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => void handleReveal(result.absolutePath)}
                    >
                      Reveal
                    </button>
                  </div>
                </article>
              ))
            )}
          </div>
        </section>
      </section>
    </main>
  )
}

function formatError(error: unknown) {
  if (typeof error === 'string') {
    return error
  }

  if (error instanceof Error) {
    return error.message
  }

  return 'Something went wrong.'
}

export default App
