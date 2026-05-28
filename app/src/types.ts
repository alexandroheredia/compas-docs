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

export type IndexFileStatus = 'indexed' | 'skipped' | 'failed'

export type IndexProgressEvent = {
  folderId: string
  phase: IndexPhase
  processedFiles: number
  totalFiles: number
  currentPath?: string | null
  fileStatus?: IndexFileStatus | null
  error?: string | null
}
