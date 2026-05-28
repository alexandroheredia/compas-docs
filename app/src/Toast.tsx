import { useCallback, useEffect, useRef, useState } from 'react'

export type ToastTone = 'info' | 'success' | 'error'

export type Toast = {
  id: number
  tone: ToastTone
  message: string
}

// Toasts are intentionally lightweight: ephemeral feedback for completed
// background work (index finished, folder removed). Persistent state still
// lives in the in-page status text so screen readers and Playwright assert
// against a single source of truth.
export function useToasts(autoDismissMs = 4200) {
  const [toasts, setToasts] = useState<Toast[]>([])
  const nextIdRef = useRef(1)
  const timersRef = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map())

  const dismiss = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id))
    const timer = timersRef.current.get(id)
    if (timer) {
      clearTimeout(timer)
      timersRef.current.delete(id)
    }
  }, [])

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

  return { toasts, push, dismiss }
}

type ToastStackProps = {
  toasts: Toast[]
  onDismiss: (id: number) => void
}

export function ToastStack({ toasts, onDismiss }: ToastStackProps) {
  if (toasts.length === 0) {
    return null
  }

  return (
    <div className="toast-stack" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div className={`toast toast-${toast.tone}`} key={toast.id}>
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
