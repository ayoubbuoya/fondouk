//! Fondouk marketplace: list a character, buy a clone, split the payment.
//!
//! The contract knows only an `avatar_id` string, a price and who gets paid.
//! The persona never touches the chain (it stays private in the backend DB).
//!
//! Listing flow (option A, HANDOFF): the owner (our backend) calls
//! `approve_listing` after checking in the DB that the creator owns the avatar
//! and that it is not a clone. Only then can the creator call `list`.

use near_sdk::store::{IterableMap, LookupMap};
use near_sdk::{
    AccountId, NearToken, Promise, env, near, near_bindgen, require,
};

const BPS_DENOMINATOR: u128 = 10_000;

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Listing {
    pub creator: AccountId,
    pub price: NearToken,
    pub active: bool,
}

/// NEP-297 events. The backend watcher reads `clone_purchased` (idempotent by tx hash).
#[near(event_json(standard = "fondouk_marketplace"))]
pub enum MarketplaceEvent {
    #[event_version("1.0.0")]
    ClonePurchased {
        avatar_id: String,
        buyer: AccountId,
        creator: AccountId,
        price: NearToken,
    },
}

#[near(contract_state)]
pub struct Marketplace {
    owner: AccountId,
    platform_account: AccountId,
    platform_fee_bps: u16,
    /// avatar_id -> creator allowed to list it (set by the owner, consumed by `list`).
    approvals: LookupMap<String, AccountId>,
    listings: IterableMap<String, Listing>,
}

impl Default for Marketplace {
    fn default() -> Self {
        env::panic_str("Marketplace must be initialised with `new`")
    }
}

#[near_bindgen]
impl Marketplace {
    #[init]
    pub fn new(owner: AccountId, platform_account: AccountId, platform_fee_bps: u16) -> Self {
        require!(
            u128::from(platform_fee_bps) <= BPS_DENOMINATOR,
            "platform_fee_bps must be <= 10000"
        );
        Self {
            owner,
            platform_account,
            platform_fee_bps,
            approvals: LookupMap::new(b"a"),
            listings: IterableMap::new(b"l"),
        }
    }

    /// Owner only: allow `creator` to list `avatar_id` once.
    pub fn approve_listing(&mut self, avatar_id: String, creator: AccountId) {
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the owner can approve listings"
        );
        self.approvals.insert(avatar_id, creator);
    }

    /// The approved creator puts the avatar on sale. The approval is consumed,
    /// so a listing removed with `unlist` needs a new approval.
    pub fn list(&mut self, avatar_id: String, price: NearToken) {
        require!(!price.is_zero(), "Price must be greater than zero");
        let caller = env::predecessor_account_id();
        require!(
            self.approvals.get(&avatar_id) == Some(&caller),
            "Listing is not approved for this account"
        );
        require!(
            !self.listings.get(&avatar_id).is_some_and(|l| l.active),
            "Already listed"
        );
        self.approvals.remove(&avatar_id);
        self.listings.insert(
            avatar_id,
            Listing {
                creator: caller,
                price,
                active: true,
            },
        );
    }

    /// Creator only: take the avatar off the shelf.
    pub fn unlist(&mut self, avatar_id: String) {
        let listing = self.listings.get_mut(&avatar_id);
        let listing = listing.unwrap_or_else(|| env::panic_str("Listing not found"));
        require!(
            env::predecessor_account_id() == listing.creator,
            "Only the creator can unlist"
        );
        listing.active = false;
    }

    /// Buy a clone. The deposit must equal the price. If the listing is in the
    /// contract map, its price is enforced. If not, the caller provides the
    /// creator account and attaches the payment directly. The platform gets
    /// `price * fee_bps / 10000`; the creator gets the rest, so any rounding
    /// remainder goes to the creator (nothing is lost).
    #[payable]
    pub fn buy_clone(&mut self, avatar_id: String, creator: Option<AccountId>) {
        let (creator_account, price) = match self.listings.get(&avatar_id) {
            Some(listing) => {
                require!(listing.active, "Listing is not active");
                require!(
                    env::predecessor_account_id() != listing.creator,
                    "Creator cannot buy their own listing"
                );
                require!(
                    env::attached_deposit() == listing.price,
                    "Attached deposit must equal the price"
                );
                (listing.creator.clone(), listing.price)
            }
            None => {
                let c = creator.unwrap_or_else(|| env::panic_str("Listing not found"));
                require!(
                    env::predecessor_account_id() != c,
                    "Creator cannot buy their own listing"
                );
                let deposit = env::attached_deposit();
                require!(!deposit.is_zero(), "Deposit must be greater than zero");
                (c, deposit)
            }
        };

        let (creator_share, platform_fee) = split_price(price, self.platform_fee_bps);

        MarketplaceEvent::ClonePurchased {
            avatar_id,
            buyer: env::predecessor_account_id(),
            creator: creator_account.clone(),
            price,
        }
        .emit();

        // Two independent transfers, not returned: NEAR rejects a joined
        // promise (`a.and(b)`) as a method result ("Returning joint promise").
        let _ = Promise::new(creator_account).transfer(creator_share);
        let _ = Promise::new(self.platform_account.clone()).transfer(platform_fee);
    }

    pub fn get_listing(&self, avatar_id: String) -> Option<Listing> {
        self.listings.get(&avatar_id).cloned()
    }

    /// Active listings only, paged by position in the map.
    pub fn get_listings(&self, from: u32, limit: u32) -> Vec<(String, Listing)> {
        self.listings
            .iter()
            .filter(|(_, l)| l.active)
            .skip(from as usize)
            .take(limit as usize)
            .map(|(id, l)| (id.clone(), l.clone()))
            .collect()
    }
}

/// Returns `(creator_share, platform_fee)`; the two always add up to `price`.
fn split_price(price: NearToken, fee_bps: u16) -> (NearToken, NearToken) {
    let total = price.as_yoctonear();
    let fee = total * u128::from(fee_bps) / BPS_DENOMINATOR;
    (
        NearToken::from_yoctonear(total - fee),
        NearToken::from_yoctonear(fee),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::mock::MockAction;
    use near_sdk::test_utils::{VMContextBuilder, get_created_receipts, get_logs};
    use near_sdk::testing_env;

    fn acc(name: &str) -> AccountId {
        name.parse().unwrap()
    }

    fn ctx(caller: &str, deposit: u128) {
        testing_env!(
            VMContextBuilder::new()
                .predecessor_account_id(acc(caller))
                .attached_deposit(NearToken::from_yoctonear(deposit))
                .build()
        );
    }

    /// Fresh contract with "avatar1" approved and listed by creator.testnet at `price`.
    fn listed(price: u128) -> Marketplace {
        ctx("owner.testnet", 0);
        let mut m = Marketplace::new(acc("owner.testnet"), acc("platform.testnet"), 1000);
        m.approve_listing("avatar1".into(), acc("creator.testnet"));
        ctx("creator.testnet", 0);
        m.list("avatar1".into(), NearToken::from_yoctonear(price));
        m
    }

    /// (receiver, amount) of every transfer created by the last call.
    fn transfers() -> Vec<(String, u128)> {
        get_created_receipts()
            .iter()
            .flat_map(|r| {
                r.actions.iter().filter_map(|a| match a {
                    MockAction::Transfer { deposit, .. } => {
                        Some((r.receiver_id.to_string(), deposit.as_yoctonear()))
                    }
                    _ => None,
                })
            })
            .collect()
    }

    #[test]
    fn split_is_exact_with_remainder_to_creator() {
        let (creator, fee) = split_price(NearToken::from_yoctonear(19), 1000);
        assert_eq!(fee.as_yoctonear(), 1); // 19 * 10% = 1.9, rounds down
        assert_eq!(creator.as_yoctonear(), 18); // creator keeps the remainder
    }

    #[test]
    fn buy_pays_creator_and_platform_and_emits_event() {
        let mut m = listed(1_000);
        ctx("buyer.testnet", 1_000);
        m.buy_clone("avatar1".into(), None);
        assert_eq!(
            transfers(),
            vec![("creator.testnet".to_string(), 900), ("platform.testnet".to_string(), 100)]
        );
        let logs = get_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].starts_with("EVENT_JSON:"));
        assert!(logs[0].contains("fondouk_marketplace"));
        assert!(logs[0].contains("clone_purchased"));
        assert!(logs[0].contains("buyer.testnet"));
    }

    #[test]
    fn buy_unlisted_with_creator_pays_and_emits_event() {
        let mut m = Marketplace::new(acc("owner.testnet"), acc("platform.testnet"), 1000);
        ctx("buyer.testnet", 2_000);
        m.buy_clone("unlisted_avatar".into(), Some(acc("creator.testnet")));
        assert_eq!(
            transfers(),
            vec![("creator.testnet".to_string(), 1800), ("platform.testnet".to_string(), 200)]
        );
        let logs = get_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains("unlisted_avatar"));
    }

    #[test]
    fn indivisible_price_gives_remainder_to_creator() {
        let mut m = listed(19);
        ctx("buyer.testnet", 19);
        m.buy_clone("avatar1".into(), None);
        assert_eq!(
            transfers(),
            vec![("creator.testnet".to_string(), 18), ("platform.testnet".to_string(), 1)]
        );
    }

    #[test]
    #[should_panic(expected = "Attached deposit must equal the price")]
    fn wrong_deposit_fails() {
        let mut m = listed(1_000);
        ctx("buyer.testnet", 999);
        m.buy_clone("avatar1".into(), None);
    }

    #[test]
    #[should_panic(expected = "Listing not found")]
    fn unlisted_avatar_fails() {
        let mut m = listed(1_000);
        ctx("buyer.testnet", 1_000);
        m.buy_clone("other".into(), None);
    }

    #[test]
    #[should_panic(expected = "Listing is not active")]
    fn unlisted_by_creator_cannot_be_bought() {
        let mut m = listed(1_000);
        ctx("creator.testnet", 0);
        m.unlist("avatar1".into());
        ctx("buyer.testnet", 1_000);
        m.buy_clone("avatar1".into(), None);
    }

    #[test]
    #[should_panic(expected = "Creator cannot buy their own listing")]
    fn creator_cannot_buy_own_listing() {
        let mut m = listed(1_000);
        ctx("creator.testnet", 1_000);
        m.buy_clone("avatar1".into(), None);
    }

    #[test]
    #[should_panic(expected = "Only the creator can unlist")]
    fn only_creator_can_unlist() {
        let mut m = listed(1_000);
        ctx("stranger.testnet", 0);
        m.unlist("avatar1".into());
    }

    #[test]
    #[should_panic(expected = "Only the owner can approve listings")]
    fn only_owner_can_approve() {
        ctx("owner.testnet", 0);
        let mut m = Marketplace::new(acc("owner.testnet"), acc("platform.testnet"), 1000);
        ctx("stranger.testnet", 0);
        m.approve_listing("avatar1".into(), acc("stranger.testnet"));
    }

    #[test]
    #[should_panic(expected = "Listing is not approved for this account")]
    fn list_without_approval_fails() {
        ctx("owner.testnet", 0);
        let mut m = Marketplace::new(acc("owner.testnet"), acc("platform.testnet"), 1000);
        ctx("creator.testnet", 0);
        m.list("avatar1".into(), NearToken::from_yoctonear(1_000));
    }

    #[test]
    #[should_panic(expected = "Listing is not approved for this account")]
    fn approval_is_consumed_by_list() {
        let mut m = listed(1_000);
        ctx("creator.testnet", 0);
        m.unlist("avatar1".into());
        m.list("avatar1".into(), NearToken::from_yoctonear(1_000)); // needs a new approval
    }

    #[test]
    fn get_listings_returns_active_only() {
        let mut m = listed(1_000);
        assert_eq!(m.get_listings(0, 10).len(), 1);
        ctx("creator.testnet", 0);
        m.unlist("avatar1".into());
        assert!(m.get_listings(0, 10).is_empty());
        assert!(m.get_listing("avatar1".into()).is_some());
    }
}
