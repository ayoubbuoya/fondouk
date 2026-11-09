/**
 * NEAR wallet login (F1) with NEAR Connect, checked by the backend (D38).
 *
 * ```text
 * login()
 *   1. POST /api/auth/challenge        -> { nonce, message, recipient }
 *   2. wallet signs (message, nonce, recipient)    (NEP-413, nothing on-chain)
 *        - no wallet connected yet:  connector.connect({ signMessageParams })
 *          = pick wallet + sign in + sign, in one popup
 *        - wallet already connected: wallet.signMessage(...)
 *   3. POST /api/auth/near             -> Set-Cookie fondouk_session (HttpOnly)
 * ```
 *
 * The page never sees the session cookie. It learns who is logged in from
 * `GET /api/auth/me` (see `currentAccount`), which also survives a reload.
 */

import { NearConnector, type SignedMessage } from '@hot-labs/near-connect'

import { createChallenge, getMe, loginNear, logoutSession } from './api'

const network = import.meta.env.VITE_NEAR_NETWORK === 'mainnet' ? 'mainnet' : 'testnet'

/** Created on first use, so importing this module has no side effect. */
let connector: NearConnector | null = null

function getConnector(): NearConnector {
  connector ??= new NearConnector({
    network,
    // Only list wallets that can sign a NEP-413 message: login needs it.
    features: { signMessage: true },
  })
  return connector
}

/** The account with a valid backend session, or `null`. */
export function currentAccount(): Promise<string | null> {
  return getMe()
}

/**
 * Full login. Resolves with the account id once the backend accepted the
 * signature. Rejects if the user closes the wallet popup or the backend
 * refuses (e.g. 401 when the key is not a full-access key of the account).
 */
export async function login(): Promise<string> {
  const challenge = await createChallenge()
  const params = {
    message: challenge.message,
    recipient: challenge.recipient,
    nonce: base64ToBytes(challenge.nonce),
  }

  const signed = await signWithWallet(params)
  const { account_id } = await loginNear({
    account_id: signed.accountId,
    public_key: signed.publicKey,
    signature: signed.signature,
    // Send the nonce exactly as the backend gave it (base64).
    nonce: challenge.nonce,
  })
  return account_id
}

/** Ends the backend session, then disconnects the wallet. */
export async function logout(): Promise<void> {
  await logoutSession()
  // Only disconnect a wallet that is really connected: `disconnect()` with no
  // wallet falls back to opening the wallet picker (near-connect 0.12.0).
  const nearConnector = getConnector()
  const connected = await nearConnector.getConnectedWallet().catch(() => null)
  if (connected) await nearConnector.disconnect(connected.wallet)
}

/** The marketplace contract the Buy button pays (must match the backend's `MARKETPLACE_CONTRACT_ID`). */
export const MARKETPLACE_CONTRACT_ID =
  import.meta.env.VITE_MARKETPLACE_CONTRACT_ID ?? 'marketplace.fondouk.testnet'

/** The vault contract that holds real avatar deposits. */
export const VAULT_CONTRACT_ID =
  import.meta.env.VITE_VAULT_CONTRACT_ID ?? 'vault.fondouk.testnet'

/** The test token contract (tUSDC) with built-in faucet. */
export const TEST_TOKEN_CONTRACT_ID =
  import.meta.env.VITE_TEST_TOKEN_CONTRACT_ID ?? 'test_token.fondouk.testnet'

/** 30 TGas: far more than `buy_clone` needs (it only emits an event and makes 2 transfers). */
const BUY_CLONE_GAS = '30000000000000'
const FT_TRANSFER_GAS = '50000000000000'

/**
 * Pays for a clone: the wallet signs `buy_clone` with the price attached. The
 * contract splits it 90% / 10% and emits the event (F11.2).
 *
 * Resolves with the transaction hash. Rejects when there is no connected
 * wallet, or the user closes the wallet popup.
 */
export async function buyClone(avatarId: string, priceYocto: string): Promise<string> {
  const connected = await getConnector()
    .getConnectedWallet()
    .catch(() => null)
  if (!connected) throw new Error('No wallet connected: log in again')

  const outcome = await connected.wallet.signAndSendTransaction({
    network,
    receiverId: MARKETPLACE_CONTRACT_ID,
    actions: [
      {
        type: 'FunctionCall',
        params: {
          methodName: 'buy_clone',
          args: { avatar_id: avatarId },
          gas: BUY_CLONE_GAS,
          deposit: priceYocto,
        },
      },
    ],
  })
  return outcome.transaction_outcome.id
}

/**
 * Claims 100 tUSDC from the test token faucet.
 */
export async function claimFaucet(): Promise<string> {
  const connected = await getConnector()
    .getConnectedWallet()
    .catch(() => null)
  if (!connected) throw new Error('No wallet connected: log in again')

  const outcome = await connected.wallet.signAndSendTransaction({
    network,
    receiverId: TEST_TOKEN_CONTRACT_ID,
    actions: [
      {
        type: 'FunctionCall',
        params: {
          methodName: 'mint_faucet',
          args: {},
          gas: BUY_CLONE_GAS,
          deposit: '0',
        },
      },
    ],
  })
  return outcome.transaction_outcome.id
}

/**
 * Deposits tUSDC tokens into the Fondouk Vault for an avatar.
 * Calls `ft_transfer_call` on the token contract with `msg: avatar_id`.
 */
export async function depositToVault(avatarId: string, amountUnits: string): Promise<string> {
  const connected = await getConnector()
    .getConnectedWallet()
    .catch(() => null)
  if (!connected) throw new Error('No wallet connected: log in again')

  const outcome = await connected.wallet.signAndSendTransaction({
    network,
    receiverId: TEST_TOKEN_CONTRACT_ID,
    actions: [
      {
        type: 'FunctionCall',
        params: {
          methodName: 'ft_transfer_call',
          args: {
            receiver_id: VAULT_CONTRACT_ID,
            amount: amountUnits,
            msg: avatarId,
          },
          gas: FT_TRANSFER_GAS,
          deposit: '1',
        },
      },
    ],
  })
  return outcome.transaction_outcome.id
}

/**
 * Withdraws deposited tUSDC from the Vault back to the owner's wallet.
 */
export async function withdrawFromVault(avatarId: string, amountUnits?: string): Promise<string> {
  const connected = await getConnector()
    .getConnectedWallet()
    .catch(() => null)
  if (!connected) throw new Error('No wallet connected: log in again')

  const outcome = await connected.wallet.signAndSendTransaction({
    network,
    receiverId: VAULT_CONTRACT_ID,
    actions: [
      {
        type: 'FunctionCall',
        params: {
          methodName: 'withdraw',
          args: {
            avatar_id: avatarId,
            amount: amountUnits ?? null,
          },
          gas: FT_TRANSFER_GAS,
          deposit: '1',
        },
      },
    ],
  })
  return outcome.transaction_outcome.id
}

type SignParams ={ message: string; recipient: string; nonce: Uint8Array }

/** Gets a NEP-413 signature, reusing an already connected wallet if any. */
async function signWithWallet(params: SignParams): Promise<SignedMessage> {
  const nearConnector = getConnector()

  const connected = await nearConnector.getConnectedWallet().catch(() => null)
  if (connected) {
    return connected.wallet.signMessage({ ...params, network })
  }

  // `connect` emits this event before it resolves (checked in the library
  // source, near-connect 0.12.0), so listening first catches the signature.
  let signed: SignedMessage | undefined
  const onSigned = (event: { accounts: { signedMessage: SignedMessage }[] }) => {
    signed = event.accounts[0]?.signedMessage
  }
  nearConnector.once('wallet:signInAndSignMessage', onSigned)
  try {
    await nearConnector.connect({ signMessageParams: params })
  } finally {
    nearConnector.off('wallet:signInAndSignMessage', onSigned)
  }
  if (!signed) throw new Error('The wallet did not return a signed message')
  return signed
}

function base64ToBytes(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0))
}
