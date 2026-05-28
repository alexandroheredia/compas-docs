export type AppView = 'main' | 'library' | 'stats'

export type FolderRecord = {
  id: string
  path: string
  displayName: string
  storagePath: string
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
