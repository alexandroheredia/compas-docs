import { useCallback, useEffect, useRef, useState } from 'react'

export type ToastTone = 'info' | 'success' | 'error'

export type Toast = {
  id: number
  tone: ToastTone
  message: string
}

// How long the toastOut CSS animation runs (must match the value in styles.css).
const EXIT_MS = 220

// Toasts are intentionally lightweight: ephemeral feedback for completed
// background work (index finished, folder removed). Persistent state still
// lives in the in-page status text so screen readers and Playwright assert
// against a single source of truth.
export function useToasts(autoDismissMs = 4200) {
  const [toasts, setToasts] = useState<Toast[]>([])
  // IDs currently playing the exit animation — CSS applies .is-exiting.
  const [exitingIds, setExitingIds] = useState<Set<number>>(new Set())
  const nextIdRef = useRef(1)
  const timersRef = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map())

  // Hard-remove from state after the exit animation finishes.
  const remove = useCallback((id: number) => {
    setToasts((current) => current.filter((t) => t.id !== id))
    setExitingIds((current) => {
      const next = new Set(current)
      next.delete(id)
      return next
    })
    timersRef.current.delete(id)
  }, [])

  // Start exit animation, then remove after EXIT_MS.
  const dismiss = useCallback((id: number) => {
    // Cancel any pending auto-dismiss timer for this id.
    const existing = timersRef.current.get(id)
    if (existing) {
      clearTimeout(existing)
    }
    setExitingIds((current) => new Set([...current, id]))
    const exitTimer = setTimeout(() => remove(id), EXIT_MS)
    timersRef.current.set(id, exitTimer)
  }, [remove])

  const push = useCallback(
    (tone: ToastTone, message: string) => {
      const id = nextIdRef.current++
      setToasts((current) => [...current, { id, tone, message }])
      const timer = setTimeout(() => dismiss(id), autoDismissMs)
      timersRef.current.set(id, timer)
      return id
    },
    [autoDismissMs, dismiss],
  )

  useEffect(() => {
    return () => {
      timersRef.current.forEach((timer) => clearTimeout(timer))
      timersRef.current.clear()
    }
  }, [])

  return { toasts, exitingIds, push, dismiss }
}

type ToastStackProps = {
  toasts: Toast[]
  exitingIds: Set<number>
  onDismiss: (id: number) => void
}

export function ToastStack({ toasts, exitingIds, onDismiss }: ToastStackProps) {
  if (toasts.length === 0) {
    return null
  }

  return (
    <div className="toast-stack" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div
          className={`toast toast-${toast.tone}${exitingIds.has(toast.id) ? ' is-exiting' : ''}`}
          key={toast.id}
        >
          <span className="toast-icon" aria-hidden="true" />
          <span>{toast.message}</span>
          <button type="button" aria-label="Dismiss notification" onClick={() => onDismiss(toast.id)}>
            <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
              <path d="m2 2 8 8M10 2l-8 8" />
            </svg>
          </button>
        </div>
      ))}
    </div>
  )
}
