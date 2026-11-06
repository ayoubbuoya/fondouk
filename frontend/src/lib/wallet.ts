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

type SignParams = { message: string; recipient: string; nonce: Uint8Array }

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
