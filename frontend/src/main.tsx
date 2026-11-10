import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
// Pixel font for titles and numbers (OFL-1.1, self-hosted by the package).
import '@fontsource/pixelify-sans/latin-400.css'
import '@fontsource/pixelify-sans/latin-600.css'
import './index.css'
import App from './App.tsx'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
