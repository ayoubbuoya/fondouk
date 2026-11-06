//! The one NEAR RPC call login needs: "is this public key a full-access key
//! of this account?" (HANDOFF D38).
//!
//! A valid signature only proves someone holds the private key. It proves the
//! account is theirs only if that key is registered on the account with
//! **full access**. A function-call key (the kind dApps leave in a browser)
//! does not count; NEP-413 says wallets sign with a full-access key.

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// Asks the RPC for the key (`query` / `view_access_key`, final block) and
/// returns `true` only when it exists on the account with `"FullAccess"`.
///
/// An unknown key or account is `Ok(false)`. The RPC reports that either as
/// a top-level `error` or as `result.error`, depending on the node version;
/// both simply have no `result.permission`.
///
/// # Errors
/// Network failure, or an answer that is not JSON.
pub async fn is_full_access_key(
    http: &reqwest::Client,
    rpc_url: &str,
    account_id: &str,
    public_key: &str,
) -> Result<bool> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": "fondouk",
        "method": "query",
        "params": {
            "request_type": "view_access_key",
            "finality": "final",
            "account_id": account_id,
            "public_key": public_key,
        }
    });
    let answer: Value = http
        .post(rpc_url)
        .json(&request)
        .send()
        .await
        .with_context(|| format!("NEAR RPC request to {rpc_url} failed"))?
        .json()
        .await
        .context("NEAR RPC answer is not JSON")?;
    Ok(answer["result"]["permission"] == "FullAccess")
}

/// Fetches a transaction with all its receipts (RPC `tx`), waiting until it is
/// final. Used to verify a marketplace purchase (Day 9, option C).
///
/// Returns `Ok(None)` when the RPC does not know the transaction (wrong hash,
/// wrong sender, or a node that has not seen it yet).
///
/// # Errors
/// Network failure, or an answer that is not JSON.
pub async fn fetch_transaction(
    http: &reqwest::Client,
    rpc_url: &str,
    tx_hash: &str,
    sender_account_id: &str,
) -> Result<Option<Value>> {
    let answer = rpc_call(
        http,
        rpc_url,
        "tx",
        json!({
            "tx_hash": tx_hash,
            "sender_account_id": sender_account_id,
            "wait_until": "FINAL",
        }),
    )
    .await?;
    Ok(answer.get("result").cloned())
}

/// Calls a read-only contract method (RPC `query` / `call_function`) and
/// returns its JSON answer.
///
/// # Errors
/// Network failure, a contract error, or an answer that is not JSON.
pub async fn view_function(
    http: &reqwest::Client,
    rpc_url: &str,
    contract_id: &str,
    method_name: &str,
    args: &Value,
) -> Result<Value> {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let answer = rpc_call(
        http,
        rpc_url,
        "query",
        json!({
            "request_type": "call_function",
            "finality": "final",
            "account_id": contract_id,
            "method_name": method_name,
            "args_base64": STANDARD.encode(args.to_string()),
        }),
    )
    .await?;
    // The RPC answers a contract failure as `result.error` (or a top-level `error`).
    let bytes = answer["result"]["result"]
        .as_array()
        .with_context(|| format!("view call {method_name} on {contract_id} failed: {answer}"))?;
    // `result.result` is the method's return value as a list of bytes (JSON text).
    let bytes: Vec<u8> = bytes
        .iter()
        .filter_map(|b| b.as_u64().and_then(|n| u8::try_from(n).ok()))
        .collect();
    serde_json::from_slice(&bytes).context("view call result is not JSON")
}

/// One JSON-RPC request; returns the whole answer object.
async fn rpc_call(
    http: &reqwest::Client,
    rpc_url: &str,
    method: &str,
    params: Value,
) -> Result<Value> {
    let request = json!({ "jsonrpc": "2.0", "id": "fondouk", "method": method, "params": params });
    http.post(rpc_url)
        .json(&request)
        .send()
        .await
        .with_context(|| format!("NEAR RPC request to {rpc_url} failed"))?
        .json()
        .await
        .context("NEAR RPC answer is not JSON")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::{Json, Router, extract::State, routing::post};

    /// A fake NEAR RPC that answers `answer` to every request. Returns its URL.
    pub(crate) async fn fake_rpc(answer: Value) -> String {
        let app = Router::new()
            .route(
                "/",
                post(
                    |State(answer): State<Value>, Json(_): Json<Value>| async move { Json(answer) },
                ),
            )
            .with_state(answer);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/")
    }

    pub(crate) fn full_access_answer() -> Value {
        json!({
            "jsonrpc": "2.0", "id": "fondouk",
            "result": { "nonce": 1, "permission": "FullAccess", "block_height": 1, "block_hash": "x" }
        })
    }

    async fn check(answer: Value) -> bool {
        let url = fake_rpc(answer).await;
        is_full_access_key(&reqwest::Client::new(), &url, "alice.testnet", "ed25519:x")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn full_access_key_is_accepted() {
        assert!(check(full_access_answer()).await);
    }

    #[tokio::test]
    async fn function_call_key_is_refused() {
        let answer = json!({
            "jsonrpc": "2.0", "id": "fondouk",
            "result": { "nonce": 1, "permission": { "FunctionCall": {
                "allowance": null, "receiver_id": "app.testnet", "method_names": []
            } } }
        });
        assert!(!check(answer).await);
    }

    #[tokio::test]
    async fn unknown_key_is_refused_in_both_error_shapes() {
        let top_level = json!({
            "jsonrpc": "2.0", "id": "fondouk",
            "error": { "name": "HANDLER_ERROR", "cause": { "name": "UNKNOWN_ACCESS_KEY" } }
        });
        let inside_result = json!({
            "jsonrpc": "2.0", "id": "fondouk",
            "result": { "error": "access key ed25519:x does not exist while viewing", "logs": [] }
        });
        assert!(!check(top_level).await);
        assert!(!check(inside_result).await);
    }

    #[tokio::test]
    async fn unreachable_rpc_is_an_error() {
        // Port 9 (discard) on localhost: nothing listens there.
        let result = is_full_access_key(
            &reqwest::Client::new(),
            "http://127.0.0.1:9/",
            "alice.testnet",
            "ed25519:x",
        )
        .await;
        assert!(result.is_err());
    }
}
