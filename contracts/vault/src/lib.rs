//! Fondouk Vault: Holds real deposited tokens (tUSDC) per avatar.
//!
//! Enforces that:
//! 1. Only approved token (e.g. tUSDC) can be deposited via `ft_transfer_call`.
//! 2. Only the avatar's owner can withdraw funds from the vault.
//! 3. Deposits are capped per avatar for safety.
//! 4. The backend operator has no permission to withdraw user principal.

use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{
    AccountId, Gas, NearToken, Promise, PromiseOrValue, env, ext_contract, near, near_bindgen,
    require,
};

const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(20);
pub const DEFAULT_MAX_DEPOSIT_PER_AVATAR: u128 = 200_000_000; // 200 tUSDC (6 decimals)

#[ext_contract(ext_ft)]
pub trait ExtFungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[near(event_json(standard = "fondouk_vault"))]
pub enum VaultEvent {
    #[event_version("1.0.0")]
    VaultDeposited {
        avatar_id: String,
        owner: AccountId,
        token: AccountId,
        amount: U128,
    },
    #[event_version("1.0.0")]
    VaultWithdrawn {
        avatar_id: String,
        owner: AccountId,
        token: AccountId,
        amount: U128,
    },
}

#[near(serializers = [json])]
pub struct AvatarDepositView {
    pub owner: AccountId,
    pub balance: U128,
    pub token: AccountId,
}

#[near(serializers = [json])]
pub struct VaultConfigView {
    pub owner: AccountId,
    pub operator: AccountId,
    pub token_id: AccountId,
    pub max_deposit_per_avatar: U128,
}

#[near(contract_state)]
pub struct Vault {
    owner: AccountId,
    operator: AccountId,
    token_id: AccountId,
    max_deposit_per_avatar: u128,
    balances: LookupMap<String, u128>,
    owners: LookupMap<String, AccountId>,
}

impl Default for Vault {
    fn default() -> Self {
        env::panic_str("Vault must be initialised with `new`")
    }
}

#[near_bindgen]
impl Vault {
    #[init]
    pub fn new(
        owner: AccountId,
        operator: AccountId,
        token_id: AccountId,
        max_deposit_per_avatar: Option<U128>,
    ) -> Self {
        require!(!env::state_exists(), "Already initialized");
        Self {
            owner,
            operator,
            token_id,
            max_deposit_per_avatar: max_deposit_per_avatar
                .map(|v| v.0)
                .unwrap_or(DEFAULT_MAX_DEPOSIT_PER_AVATAR),
            balances: LookupMap::new(b"b"),
            owners: LookupMap::new(b"o"),
        }
    }

    /// Deposit handler called by the Fungible Token contract during `ft_transfer_call`.
    ///
    /// `msg` MUST contain the `avatar_id` UUID string.
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        require!(
            env::predecessor_account_id() == self.token_id,
            "Only approved token accepted"
        );
        let avatar_id = msg.trim().to_string();
        require!(!avatar_id.is_empty(), "avatar_id must not be empty");

        // If the avatar already has an owner recorded, the depositor must match.
        if let Some(existing_owner) = self.owners.get(&avatar_id) {
            require!(
                existing_owner == &sender_id,
                "Only the avatar owner can deposit into this avatar"
            );
        } else {
            self.owners.insert(avatar_id.clone(), sender_id.clone());
        }

        let current_balance = self.balances.get(&avatar_id).copied().unwrap_or(0);
        let new_balance = current_balance.checked_add(amount.0).expect("Balance overflow");
        require!(
            new_balance <= self.max_deposit_per_avatar,
            "Deposit exceeds max deposit cap"
        );

        self.balances.insert(avatar_id.clone(), new_balance);

        VaultEvent::VaultDeposited {
            avatar_id,
            owner: sender_id,
            token: self.token_id.clone(),
            amount,
        }
        .emit();

        // 0 refund: all tokens accepted
        PromiseOrValue::Value(U128(0))
    }

    /// Withdraw funds from an avatar back to the owner's wallet.
    ///
    /// ONLY the owner can withdraw.
    #[payable]
    pub fn withdraw(&mut self, avatar_id: String, amount: Option<U128>) -> Promise {
        require!(
            env::attached_deposit() == NearToken::from_yoctonear(1),
            "Requires attached deposit of exactly 1 yoctoNEAR"
        );
        let caller = env::predecessor_account_id();
        let owner = self
            .owners
            .get(&avatar_id)
            .cloned()
            .unwrap_or_else(|| env::panic_str("Avatar not found in vault"));
        require!(caller == owner, "Only the avatar owner can withdraw");

        let current_balance = self
            .balances
            .get(&avatar_id)
            .copied()
            .unwrap_or_else(|| env::panic_str("No balance for avatar"));
        let withdraw_amount = amount.map(|a| a.0).unwrap_or(current_balance);
        require!(withdraw_amount > 0, "Withdraw amount must be greater than zero");
        require!(
            current_balance >= withdraw_amount,
            "Insufficient avatar balance"
        );

        self.balances
            .insert(avatar_id.clone(), current_balance - withdraw_amount);

        VaultEvent::VaultWithdrawn {
            avatar_id,
            owner: caller.clone(),
            token: self.token_id.clone(),
            amount: U128(withdraw_amount),
        }
        .emit();

        // Transfer tokens to owner
        ext_ft::ext(self.token_id.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(caller, U128(withdraw_amount), Some("vault withdrawal".to_string()))
    }

    // --- Views ---

    pub fn get_avatar_deposit(&self, avatar_id: String) -> Option<AvatarDepositView> {
        let owner = self.owners.get(&avatar_id)?.clone();
        let balance = self.balances.get(&avatar_id).copied().unwrap_or(0);
        Some(AvatarDepositView {
            owner,
            balance: U128(balance),
            token: self.token_id.clone(),
        })
    }

    pub fn get_config(&self) -> VaultConfigView {
        VaultConfigView {
            owner: self.owner.clone(),
            operator: self.operator.clone(),
            token_id: self.token_id.clone(),
            max_deposit_per_avatar: U128(self.max_deposit_per_avatar),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn admin() -> AccountId {
        "admin.testnet".parse().unwrap()
    }

    fn operator() -> AccountId {
        "operator.testnet".parse().unwrap()
    }

    fn token() -> AccountId {
        "token.testnet".parse().unwrap()
    }

    fn alice() -> AccountId {
        "alice.testnet".parse().unwrap()
    }

    fn bob() -> AccountId {
        "bob.testnet".parse().unwrap()
    }

    #[test]
    fn test_deposit_and_view() {
        let mut context = VMContextBuilder::new();
        testing_env!(context.build());

        let mut vault = Vault::new(admin(), operator(), token(), None);

        // Deposit from token contract
        testing_env!(context.predecessor_account_id(token()).build());
        let res = vault.ft_on_transfer(alice(), U128(50_000_000), "avatar-123".to_string());
        match res {
            PromiseOrValue::Value(unused) => assert_eq!(unused.0, 0),
            _ => panic!("Expected Value"),
        }

        let dep = vault.get_avatar_deposit("avatar-123".to_string()).unwrap();
        assert_eq!(dep.owner, alice());
        assert_eq!(dep.balance.0, 50_000_000);
        assert_eq!(dep.token, token());
    }

    #[test]
    #[should_panic(expected = "Only approved token accepted")]
    fn test_deposit_wrong_token_fails() {
        let mut context = VMContextBuilder::new();
        testing_env!(context.predecessor_account_id(bob()).build());

        let mut vault = Vault::new(admin(), operator(), token(), None);
        let _ = vault.ft_on_transfer(alice(), U128(50_000_000), "avatar-123".to_string());
    }

    #[test]
    #[should_panic(expected = "Only the avatar owner can deposit into this avatar")]
    fn test_deposit_different_owner_fails() {
        let mut context = VMContextBuilder::new();
        let mut vault = Vault::new(admin(), operator(), token(), None);

        // Alice deposits first
        testing_env!(context.predecessor_account_id(token()).build());
        let _ = vault.ft_on_transfer(alice(), U128(50_000_000), "avatar-123".to_string());

        // Bob tries to deposit into Alice's avatar
        let _ = vault.ft_on_transfer(bob(), U128(10_000_000), "avatar-123".to_string());
    }

    #[test]
    #[should_panic(expected = "Deposit exceeds max deposit cap")]
    fn test_deposit_over_cap_fails() {
        let mut context = VMContextBuilder::new();
        testing_env!(context.predecessor_account_id(token()).build());

        let mut vault = Vault::new(admin(), operator(), token(), Some(U128(50_000_000)));
        let _ = vault.ft_on_transfer(alice(), U128(60_000_000), "avatar-123".to_string());
    }

    #[test]
    #[should_panic(expected = "Only the avatar owner can withdraw")]
    fn test_non_owner_cannot_withdraw() {
        let mut context = VMContextBuilder::new();
        let mut vault = Vault::new(admin(), operator(), token(), None);

        testing_env!(context.predecessor_account_id(token()).build());
        let _ = vault.ft_on_transfer(alice(), U128(50_000_000), "avatar-123".to_string());

        // Bob tries to withdraw
        testing_env!(context
            .predecessor_account_id(bob())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        let _ = vault.withdraw("avatar-123".to_string(), None);
    }
}
