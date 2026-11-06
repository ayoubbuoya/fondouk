import type { ReactNode } from 'react'

interface SidePanelProps {
  onClose: () => void
  children: ReactNode
}

/** The frame on the right of the town: a close button and scrollable content. */
export function SidePanel({ onClose, children }: SidePanelProps) {
  return (
    <aside className="side-panel" aria-label="Details">
      <button className="side-panel-close" onClick={onClose} aria-label="Close">
        ×
      </button>
      {children}
    </aside>
  )
}
