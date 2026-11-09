# NEAR CLI (`near-cli-rs`) Guide & Playbook

This document captures the exact, tested commands for `near-cli-rs` (v0.30+) used in the Fondouk project.

Unlike the legacy Node.js `near-cli` (e.g. `near view`, `near call`, `near deploy`), the modern Rust-based CLI (`near-cli-rs`) uses structured subcommands, flags, and keychain signing options.

---

## 1. Account Management

### View Account Summary & Balance
Check native balance, storage usage, and deployed contract code:
```bash
near account view-account-summary <account_id> network-config testnet now
```
*Example:*
```bash
near account view-account-summary fondouk.testnet network-config testnet now
```

### Create a Subaccount (Funded by Parent Account)
Creates a subaccount (e.g. `<subname>.<parent>.testnet`), generates a new Ed25519 keypair, stores it in the local OS keychain, and transfers initial balance:
```bash
near account create-account fund-myself <subaccount_id> '<amount> NEAR' autogenerate-new-keypair --signature-scheme ed25519 save-to-keychain sign-as <parent_account_id> network-config testnet sign-with-keychain send
```
*Example:*
```bash
near account create-account fund-myself test_token.fondouk.testnet '1 NEAR' autogenerate-new-keypair --signature-scheme ed25519 save-to-keychain sign-as fondouk.testnet network-config testnet sign-with-keychain send
```

---

## 2. Token Transfers

### Transfer NEAR Between Accounts
> **Important Syntax Rule:** The sender account ID goes right after `near tokens`, **not** in a `--sign-as` flag.
```bash
near tokens <sender_account_id> send-near <receiver_account_id> '<amount> NEAR' network-config testnet sign-with-keychain send
```
*Example:*
```bash
near tokens fondouk.testnet send-near test_token.fondouk.testnet '1 NEAR' network-config testnet sign-with-keychain send
```

---

## 3. Contract Deployment & Initialization

### Deploy with an `#[init]` Call
Deploys the compiled `.wasm` binary and immediately invokes the initialization function:
> **Note:** The contract account itself is the signer by default. In PowerShell, always quote the JSON arguments to prevent shell expansion (e.g. `'{}'`).
```bash
near contract deploy <contract_account_id> use-file <path_to_wasm> with-init-call <init_function_name> json-args '<json_args>' prepaid-gas '<gas_amount>' attached-deposit '<deposit_near>' network-config testnet sign-with-keychain send
```
*Examples:*
1. **Empty arguments (e.g. `new()`):**
   ```powershell
   near contract deploy test_token.fondouk.testnet use-file contracts/marketplace/target/wasm32-unknown-unknown/release/test_token.wasm with-init-call new json-args '{}' prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config testnet sign-with-keychain send
   ```
2. **With JSON arguments:**
   ```powershell
   near contract deploy vault.fondouk.testnet use-file contracts/marketplace/target/wasm32-unknown-unknown/release/vault.wasm with-init-call new json-args '{\"owner\":\"owner.fondouk.testnet\",\"operator\":\"owner.fondouk.testnet\",\"token_id\":\"test_token.fondouk.testnet\"}' prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config testnet sign-with-keychain send
   ```

### Storage Staking Requirement
NEAR requires storage staking of **1 NEAR per 100 KB** of state and contract bytecode.
- If an account only has 1 NEAR balance and tries to deploy a ~400 KB contract, the transaction will fail with:
  `Receipt action can't be completed, because the remaining balance will not be enough to cover storage.`
- Ensure the contract account is funded with at least **3 to 4 NEAR** prior to deployment.

---

## 4. Contract Interaction

### Read-Only View Calls
Queries a contract state or view function without spending gas or signing:
> **Caution:** Always pass `json-args '{}'` even for functions with no arguments. Passing `text-args ""` without input can cause the CLI to hang waiting on interactive stdin.
```bash
near contract call-function as-read-only <contract_account_id> <function_name> json-args '<json_args>' network-config testnet now
```
*Examples:*
```bash
# Query token metadata
near contract call-function as-read-only test_token.fondouk.testnet ft_metadata json-args '{}' network-config testnet now

# Query vault configuration
near contract call-function as-read-only vault.fondouk.testnet get_config json-args '{}' network-config testnet now
```

### Mutating Function Calls (Transactions)
Executes a state-changing method signed by a user account:
```bash
near contract call-function as-transaction <contract_account_id> <function_name> json-args '<json_args>' prepaid-gas '<gas>' attached-deposit '<deposit>' sign-as <signer_account_id> network-config testnet sign-with-keychain send
```
*Example (Claiming 100 tUSDC from the test token faucet):*
```bash
near contract call-function as-transaction test_token.fondouk.testnet mint_faucet json-args '{}' prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as demo.fondouk.testnet network-config testnet sign-with-keychain send
```

---

## 5. Local Credentials & Keys Storage
On Windows, `near-cli-rs` tracks registered accounts in:
- Account registry: `%USERPROFILE%\.near-credentials\accounts.json`
- Private keys: Windows Credential Manager / Keychain (`sign-with-keychain`)
