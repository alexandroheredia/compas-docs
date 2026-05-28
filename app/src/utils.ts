import { invoke } from '@tauri-apps/api/core'
import type { AppView, StatusTone } from './types'

export const hasTauriInvoke = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
export const NAVIGATE_EVENT = 'app:navigate'
export const INDEX_PROGRESS_EVENT = 'index:progress'

export async function openView(view: AppView, currentView?: AppView) {
  if (view === currentView) {
    return
  }

  if (hasTauriInvoke) {
    await invoke('navigate_main_window', { view })
    return
  }

  const url = new URL(window.location.href)
  url.searchParams.set('window', view)
  window.history.pushState({ window: view }, '', url)
  window.dispatchEvent(new PopStateEvent('popstate'))
}

export function formatIndexedAt(value: number | null) {
  if (value === null) {
    return 'Not indexed yet'
  }

  return new Date(value * 1000).toLocaleString([], {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  })
}

export function formatRelativeTime(value: number | null) {
  if (value === null) {
    return '—'
  }

  const deltaSeconds = Math.max(0, Math.round(Date.now() / 1000) - value)
  if (deltaSeconds < 60) return 'Just now'
  if (deltaSeconds < 3600) return `${Math.floor(deltaSeconds / 60)}m ago`
  if (deltaSeconds < 86400) return `${Math.floor(deltaSeconds / 3600)}h ago`
  return `${Math.floor(deltaSeconds / 86400)}d ago`
}

export function compactPath(path: string) {
  const normalized = path.split('/').filter(Boolean)
  if (normalized.length <= 4) return path
  return `…/${normalized.slice(-3).join('/')}`
}

export function formatCount(value: number) {
  return value.toLocaleString()
}

export function formatFolderCount(value: number) {
  return `${formatCount(value)} folder${value === 1 ? '' : 's'}`
}

export function statusTone(status: string, busy: boolean): StatusTone {
  if (busy) return 'busy'
  if (/(fail|error|not found|unsupported|wrong|corrupted|incomplete|invalid)/i.test(status)) {
    return 'error'
  }
  return 'idle'
}

export function formatError(error: unknown) {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Something went wrong.'
}

/** Short, human-readable basename of a path for inline status text. */
export function basename(path: string) {
  if (!path) return ''
  const parts = path.split('/').filter(Boolean)
  return parts[parts.length - 1] ?? path
}
