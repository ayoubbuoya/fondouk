//! NEP-141 Fungible Token with Faucet for Fondouk (tUSDC).
//!
//! Provides `mint_faucet` so anyone on testnet can claim 100 tUSDC for testing.

use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{
    AccountId, Gas, NearToken, PromiseOrValue, PromiseResult, env, ext_contract, near,
    near_bindgen, require,
};

pub const FAUCET_AMOUNT: u128 = 100_000_000; // 100 tUSDC (6 decimals)
const GAS_FOR_FT_ON_TRANSFER: Gas = Gas::from_tgas(30);
const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas::from_tgas(15);

#[ext_contract(ext_ft_receiver)]
pub trait ExtFtReceiver {
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128>;
}

#[ext_contract(ext_self)]
pub trait ExtSelf {
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128;
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct FungibleTokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<String>,
    pub decimals: u8,
}

#[near(serializers = [json])]
pub struct StorageBalance {
    pub total: NearToken,
    pub available: NearToken,
}

#[near(event_json(standard = "nep141"))]
pub enum FtEvent {
    #[event_version("1.0.0")]
    FtMint {
        owner_id: AccountId,
        amount: U128,
        memo: Option<String>,
    },
    #[event_version("1.0.0")]
    FtTransfer {
        old_owner_id: AccountId,
        new_owner_id: AccountId,
        amount: U128,
        memo: Option<String>,
    },
}

#[near(contract_state)]
pub struct TestToken {
    accounts: LookupMap<AccountId, u128>,
    total_supply: u128,
    metadata: FungibleTokenMetadata,
}

impl Default for TestToken {
    fn default() -> Self {
        env::panic_str("TestToken must be initialised with `new`")
    }
}

#[near_bindgen]
impl TestToken {
    #[init]
    pub fn new() -> Self {
        require!(!env::state_exists(), "Already initialized");
        Self {
            accounts: LookupMap::new(b"a"),
            total_supply: 0,
            metadata: FungibleTokenMetadata {
                spec: "ft-1.0.0".to_string(),
                name: "Fondouk Test USD".to_string(),
                symbol: "tUSDC".to_string(),
                icon: None,
                reference: None,
                reference_hash: None,
                decimals: 6,
            },
        }
    }

    /// Faucet claim: mints 100 tUSDC to caller.
    pub fn mint_faucet(&mut self) -> U128 {
        let caller = env::predecessor_account_id();
        let balance = self.accounts.get(&caller).copied().unwrap_or(0);
        let new_balance = balance.checked_add(FAUCET_AMOUNT).expect("Overflow");
        self.accounts.insert(caller.clone(), new_balance);
        self.total_supply = self.total_supply.checked_add(FAUCET_AMOUNT).expect("Supply overflow");

        FtEvent::FtMint {
            owner_id: caller,
            amount: U128(FAUCET_AMOUNT),
            memo: Some("faucet claim".to_string()),
        }
        .emit();

        U128(FAUCET_AMOUNT)
    }

    // --- NEP-141 Core ---

    #[payable]
    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        require!(
            env::attached_deposit() == NearToken::from_yoctonear(1),
            "Requires attached deposit of exactly 1 yoctoNEAR"
        );
        let sender_id = env::predecessor_account_id();
        self.internal_transfer(&sender_id, &receiver_id, amount.0, memo);
    }

    #[payable]
    pub fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        require!(
            env::attached_deposit() == NearToken::from_yoctonear(1),
            "Requires attached deposit of exactly 1 yoctoNEAR"
        );
        let sender_id = env::predecessor_account_id();
        self.internal_transfer(&sender_id, &receiver_id, amount.0, memo);

        ext_ft_receiver::ext(receiver_id.clone())
            .with_static_gas(GAS_FOR_FT_ON_TRANSFER)
            .ft_on_transfer(sender_id.clone(), amount, msg)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                    .ft_resolve_transfer(sender_id, receiver_id, amount),
            )
            .into()
    }

    pub fn ft_total_supply(&self) -> U128 {
        U128(self.total_supply)
    }

    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        U128(self.accounts.get(&account_id).copied().unwrap_or(0))
    }

    pub fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.clone()
    }

    // --- Storage Management (simple compatibility) ---

    #[payable]
    pub fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        _registration_only: Option<bool>,
    ) -> StorageBalance {
        let account = account_id.unwrap_or_else(env::predecessor_account_id);
        if !self.accounts.contains_key(&account) {
            self.accounts.insert(account, 0);
        }
        StorageBalance {
            total: env::attached_deposit(),
            available: env::attached_deposit(),
        }
    }

    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        if self.accounts.contains_key(&account_id) {
            Some(StorageBalance {
                total: NearToken::from_yoctonear(0),
                available: NearToken::from_yoctonear(0),
            })
        } else {
            None
        }
    }

    // --- Private callbacks ---

    #[private]
    #[allow(deprecated)]
    pub fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        let unused_amount = match env::promise_result(0) {
            PromiseResult::Successful(value) => {
                if let Ok(unused) = near_sdk::serde_json::from_slice::<U128>(&value) {
                    std::cmp::min(amount.0, unused.0)
                } else {
                    amount.0
                }
            }
            PromiseResult::Failed => amount.0,
        };

        if unused_amount > 0 {
            let receiver_balance = self.accounts.get(&receiver_id).copied().unwrap_or(0);
            if receiver_balance > 0 {
                let refund = std::cmp::min(receiver_balance, unused_amount);
                self.accounts.insert(receiver_id.clone(), receiver_balance - refund);
                let sender_balance = self.accounts.get(&sender_id).copied().unwrap_or(0);
                self.accounts.insert(sender_id.clone(), sender_balance + refund);
                FtEvent::FtTransfer {
                    old_owner_id: receiver_id,
                    new_owner_id: sender_id,
                    amount: U128(refund),
                    memo: Some("refund".to_string()),
                }
                .emit();
            }
        }

        U128(unused_amount)
    }

    fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        amount: u128,
        memo: Option<String>,
    ) {
        require!(sender_id != receiver_id, "Sender and receiver must differ");
        require!(amount > 0, "Amount must be positive");
        let sender_balance = self
            .accounts
            .get(sender_id)
            .copied()
            .unwrap_or(0);
        require!(sender_balance >= amount, "Not enough balance");
        self.accounts.insert(sender_id.clone(), sender_balance - amount);
        let receiver_balance = self
            .accounts
            .get(receiver_id)
            .copied()
            .unwrap_or(0);
        self.accounts.insert(receiver_id.clone(), receiver_balance + amount);

        FtEvent::FtTransfer {
            old_owner_id: sender_id.clone(),
            new_owner_id: receiver_id.clone(),
            amount: U128(amount),
            memo,
        }
        .emit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn alice() -> AccountId {
        "alice.testnet".parse().unwrap()
    }

    fn bob() -> AccountId {
        "bob.testnet".parse().unwrap()
    }

    #[test]
    fn test_faucet_mints_correct_amount() {
        let mut context = VMContextBuilder::new();
        testing_env!(context.predecessor_account_id(alice()).build());

        let mut token = TestToken::new();
        assert_eq!(token.ft_balance_of(alice()).0, 0);

        let minted = token.mint_faucet();
        assert_eq!(minted.0, FAUCET_AMOUNT);
        assert_eq!(token.ft_balance_of(alice()).0, FAUCET_AMOUNT);
        assert_eq!(token.ft_total_supply().0, FAUCET_AMOUNT);

        // Second mint
        token.mint_faucet();
        assert_eq!(token.ft_balance_of(alice()).0, FAUCET_AMOUNT * 2);
    }

    #[test]
    fn test_transfer() {
        let mut context = VMContextBuilder::new();
        testing_env!(context.predecessor_account_id(alice()).build());

        let mut token = TestToken::new();
        token.mint_faucet();

        testing_env!(context
            .predecessor_account_id(alice())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());

        token.ft_transfer(bob(), U128(30_000_000), None);
        assert_eq!(token.ft_balance_of(alice()).0, 70_000_000);
        assert_eq!(token.ft_balance_of(bob()).0, 30_000_000);
    }

    #[test]
    #[should_panic(expected = "Not enough balance")]
    fn test_transfer_insufficient_balance() {
        let mut context = VMContextBuilder::new();
        testing_env!(context
            .predecessor_account_id(alice())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());

        let mut token = TestToken::new();
        token.ft_transfer(bob(), U128(10_000_000), None);
    }
}
