/// <reference types="vite/client" />

// The VITE_* variables this app reads (see .env.example).
interface ImportMetaEnv {
  readonly VITE_NEAR_NETWORK?: 'testnet' | 'mainnet'
  readonly VITE_WS_URL?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
