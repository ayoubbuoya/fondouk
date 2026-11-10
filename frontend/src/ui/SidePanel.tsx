import type { ReactNode } from 'react'

interface SidePanelProps {
  onClose: () => void
  className?: string
  children: ReactNode
}

/** The frame on the right of the town: a close button and scrollable content. */
export function SidePanel({ onClose, className, children }: SidePanelProps) {
  return (
    <aside className={`side-panel ${className ?? ''}`.trim()} aria-label="Details">
      <button className="side-panel-close" onClick={onClose} aria-label="Close">
        ×
      </button>
      {children}
    </aside>
  )
}
