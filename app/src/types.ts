export type AppView = 'main' | 'library' | 'stats'

export type FolderRecord = {
  id: string
  path: string
  displayName: string
  storagePath: string
  fileTypes: string[]
  lastIndexedAt: number | null
  watchEnabled: boolean
}

export type SearchDocumentItem = {
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

export type RemoveFolderResponse = {
  removed: boolean
}

export type LibraryStats = {
  folderCount: number
  indexedFolderCount: number
  documentCount: number
  chunkCount: number
  lastIndexedAt: number | null
}

export type StatusTone = 'idle' | 'busy' | 'error'

export type IndexPhase = 'started' | 'file' | 'finalizing' | 'completed' | 'failed'

export type IndexFileStatus = 'indexed' | 'unchanged' | 'skipped' | 'failed'

export type IndexProgressEvent = {
  folderId: string
  phase: IndexPhase
  processedFiles: number
  totalFiles: number
  currentPath?: string | null
  fileStatus?: IndexFileStatus | null
  error?: string | null
}

export type WatchStatusPhase =
  | 'started'
  | 'change-detected'
  | 'remove-detected'
  | 'reindex-started'
  | 'reindex-completed'
  | 'reindex-failed'
  | 'stopped'

export type WatchStatusEvent = {
  folderId: string
  phase: WatchStatusPhase
  path?: string | null
  error?: string | null
}

export type WatchFolderResponse = {
  folder: FolderRecord
}

/// One chunk of an indexed file, returned by the `read_document_chunks` command.
/// `preview` is the same value stored on SearchDocumentItem so the file viewer
/// can locate and scroll to the matched chunk by a simple equality check.
export type FileChunk = {
  chunkId: string
  headingPath: string[]
  pageStart: number | null
  pageEnd: number | null
  text: string
  preview: string
}
