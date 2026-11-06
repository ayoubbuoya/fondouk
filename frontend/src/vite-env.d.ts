/// <reference types="vite/client" />

// The VITE_* variables this app reads (see .env.example).
interface ImportMetaEnv {
  readonly VITE_NEAR_NETWORK?: 'testnet' | 'mainnet'
  readonly VITE_WS_URL?: string
  readonly VITE_MARKETPLACE_CONTRACT_ID?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
