#![no_std]

mod admin;
mod batch;
mod errors;
mod events;
mod fee;
mod grace;
mod merchant_stats;
mod migration;
mod min_interval;
mod referral;
mod spending_limit;
mod storage;
mod subscription_count;
mod subscription_history;
mod subscription_metadata;
mod test;
mod trial;
mod validation;
mod whitelist;

use crate::errors::ContractError;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, String, Symbol, Vec};

pub use batch::ChargeResult;

// ─────────────────────────────────────────────────────────────
// Storage keys
// ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Subscription(Address),
    Token,
    // Admin
    Admin,
    // Grace period
    GracePeriod,
    // Merchant whitelist
    MerchantWhitelist(Address),
    WhitelistEnabled,
    // Protocol fee
    FeeCollector,
    FeeBps,
    // Feature: subscription count
    ActiveCount,
    // Feature: merchant revenue stats
    MerchantRevenue(Address),
    // Per-day merchant revenue buckets (keyed by Unix day)
    MerchantRevenueDay(Address, u64),
    // Feature: daily spending limits (temporary storage)
    DailyLimit(Address),
    DailySpent(Address),
    // Feature: referral tracking
    Referral(Address),
    // Feature: state migration
    SchemaVersion,
    // Feature: subscription metadata labels
    SubscriptionMeta(Address),
    // Feature: charge history
    ChargeHistory(Address),
    // Feature: contract-level pause
    ContractPaused,
    // Feature: minimum subscription interval floor
    MinInterval,
    // Feature: consolidated merchant revenue history (Vec<i128>)
    MerchantRevenueHistory(Address),
    // Feature: subscriber index (append-only log)
    SubscriberIndex(u64),
    SubscriberIndexSize,
}

// ─────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────

pub const SUBSCRIPTION_TTL_LEDGERS: u32 = 6307200; // ~1 year (assuming 5s blocks)

// ─────────────────────────────────────────────────────────────
// Data types
// ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Subscription {
    pub merchant: Address,
    pub amount: i128,
    pub interval: u64,
    pub last_charged: u64,
    pub active: bool,
    pub paused: bool,
    pub token: Address,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct HealthReport {
    pub is_healthy: bool,
    pub contract_paused: bool,
    pub token_configured: bool,
    pub admin_configured: bool,
    pub instance_ttl_ledgers: u32,
    pub active_subscription_count: u64,
    pub schema_version: u32,
}

// ─────────────────────────────────────────────────────────────
// Contract
// ─────────────────────────────────────────────────────────────

#[contract]
pub struct FlowPay;

#[contractimpl]
impl FlowPay {
    pub fn initialize(env: Env, token: Address) {
        if env.storage().instance().has(&DataKey::Token) {
            panic!("already initialized");
        }

        env.storage().instance().set(&DataKey::Token, &token);
    }

    pub fn subscribe(
        env: Env,
        user: Address,
        merchant: Address,
        amount: i128,
        interval: u64,
        token: Address,
        trial_period: Option<u64>,
        referrer: Option<Address>,
    ) {
        user.require_auth();

        if whitelist::is_whitelist_enabled(&env) {
            if !whitelist::is_whitelisted(&env, &merchant) {
                env.panic_with_error(ContractError::MerchantNotWhitelisted);
            }
        }

        assert!(amount > 0, "amount must be positive");
        assert!(interval > 0, "interval must be positive");

        if interval < min_interval::get_min_interval(&env) {
            env.panic_with_error(ContractError::IntervalTooShort);
        }

        let token_client = token::Client::new(&env, &token);
        let allowance = token_client.allowance(&user, &env.current_contract_address());
        assert!(allowance >= amount, "insufficient allowance");

        let now = env.ledger().timestamp();
        let last_charged = match trial_period {
            Some(period) => now + period,
            None => now,
        };

        let existing = storage::get_subscription(&env, &user);
        let should_increment = existing.as_ref().map_or(true, |s| !s.active);

        let sub = Subscription {
            merchant,
            amount,
            interval,
            last_charged,
            active: true,
            paused: false,
            token,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(user.clone()), &sub);

        extend_subscription_ttl(&env, &user);

        if should_increment {
            subscription_count::increment(&env);
            subscription_count::append_subscriber_index(&env, &user);
        }
        referral::store_referral(&env, &user, &referrer);
        events::publish_subscribed(&env, &user, &sub);
    }

    pub fn charge(env: Env, user: Address) {
        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(ContractError::NoSubscriptionFound));

        assert!(sub.active, "subscription is not active");
        assert!(!sub.paused, "subscription is paused");

        let now = env.ledger().timestamp();

        if now < sub.last_charged + sub.interval {
            env.panic_with_error(ContractError::IntervalNotElapsed);
        }

        let grace_period = grace::get_grace_period(&env);
        if grace_period > 0 && now > sub.last_charged + sub.interval + grace_period {
            env.panic_with_error(ContractError::GracePeriodElapsed);
        }

        let token = token::Client::new(&env, &sub.token);

        token.transfer_from(
            &env.current_contract_address(),
            &user,
            &sub.merchant,
            &sub.amount,
        );

        merchant_stats::increment_revenue_with_daily(&env, &sub.merchant, sub.amount);

        sub.last_charged = now;

        env.storage().persistent().set(&key, &sub);
        extend_subscription_ttl(&env, &user);

        subscription_history::record_charge(&env, &user, now);
        events::publish_charged(&env, &user, &sub, now);
    }

    pub fn extend_subscription_ttl(env: Env, user: Address) {
        extend_subscription_ttl(&env, &user);
    }

    pub fn pay_per_use(env: Env, user: Address, amount: i128) {
        user.require_auth();

        assert!(amount > 0, "amount must be positive");

        let key = DataKey::Subscription(user.clone());

        let sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        assert!(sub.active, "subscription is not active");
        assert!(!sub.paused, "subscription is paused");

        spending_limit::enforce_limit(&env, &user, amount);

        let token = token::Client::new(&env, &sub.token);

        token.transfer_from(
            &env.current_contract_address(),
            &user,
            &sub.merchant,
            &amount,
        );

        merchant_stats::increment_revenue_with_daily(&env, &sub.merchant, amount);
        spending_limit::record_spend(&env, &user, amount);

        events::publish_pay_per_use(&env, &user, &sub.merchant, amount);
    }

    pub fn cancel(env: Env, user: Address) {
        user.require_auth();

        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        sub.active = false;

        env.storage().persistent().set(&key, &sub);

        subscription_count::decrement(&env);
        events::publish_cancelled(&env, &user);
    }

    pub fn pause(env: Env, user: Address) {
        user.require_auth();

        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        assert!(sub.active, "subscription is not active");

        sub.paused = true;

        env.storage().persistent().set(&key, &sub);

        env.events()
            .publish((Symbol::new(&env, "paused"), user), ());
    }

    pub fn resume(env: Env, user: Address) {
        user.require_auth();

        let key = DataKey::Subscription(user.clone());

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        assert!(sub.active, "subscription is not active");

        sub.paused = false;

        env.storage().persistent().set(&key, &sub);

        env.events()
            .publish((Symbol::new(&env, "resumed"), user), ());
    }

    pub fn get_subscription(env: Env, user: Address) -> Option<Subscription> {
        env.storage().persistent().get(&DataKey::Subscription(user))
    }

    /// Returns the Unix timestamp of the next scheduled charge for a user.
    ///
    /// Returns `None` if:
    /// - No subscription exists for the user
    /// - The subscription is inactive (cancelled)
    ///
    /// Returns `Some(last_charged + interval)` if the subscription is active.
    pub fn next_charge_at(env: Env, user: Address) -> Option<u64> {
        let sub = storage::get_subscription(&env, &user)?;
        if !sub.active {
            None
        } else {
            Some(sub.last_charged + sub.interval)
        }
    }

    /// Returns the trial end timestamp if the user is in a trial period.
    pub fn get_trial_end(env: Env, user: Address) -> Option<u64> {
        trial::get_trial_end(env, user)
    }

    /// Sets the contract-wide grace period for charges.
    /// Only the contract admin can call this.
    pub fn set_grace_period(env: Env, seconds: u64) {
        admin::require_admin(&env);
        grace::set_grace_period(&env, seconds);
    }

    /// Sets the minimum allowed subscription interval in seconds.
    /// Only the contract admin can call this. Panics if seconds == 0.
    pub fn set_min_interval(env: Env, seconds: u64) {
        assert!(seconds > 0, "min interval must be positive");
        admin::require_admin(&env);
        min_interval::set_min_interval(&env, seconds);
        events::publish_min_interval_updated(&env, seconds);
    }

    /// Returns the minimum allowed subscription interval in seconds.
    /// Defaults to 3600 (1 hour) when unset.
    pub fn get_min_interval(env: Env) -> u64 {
        min_interval::get_min_interval(&env)
    }

    /// Adds a merchant to the whitelist.
    pub fn add_merchant(env: Env, merchant: Address) {
        admin::require_admin(&env);
        whitelist::add_merchant(&env, &merchant);
    }

    /// Removes a merchant from the whitelist.
    pub fn remove_merchant(env: Env, merchant: Address) {
        admin::require_admin(&env);
        whitelist::remove_merchant(&env, &merchant);
    }

    /// Enables or disables the merchant whitelist.
    pub fn set_whitelist_enabled(env: Env, enabled: bool) {
        admin::require_admin(&env);
        whitelist::set_whitelist_enabled(&env, enabled);
    }

    /// Sets the protocol fee collection settings.
    /// Only the contract admin can call this.
    pub fn set_fee(env: Env, collector: Address, bps: u32) {
        admin::require_admin(&env);
        fee::set_fee(&env, collector, bps);
    }

    // ─────────────────────────────────────────────────────────────
    // Batch charge
    // ─────────────────────────────────────────────────────────────

    /// Charges multiple subscribers in a single transaction.
    ///
    /// Each user is processed independently — individual failures (inactive,
    /// paused, interval not elapsed, etc.) are recorded as a `ChargeResult`
    /// variant and do **not** abort the batch.
    pub fn batch_charge(env: Env, users: Vec<Address>) -> Vec<ChargeResult> {
        batch::batch_charge(&env, users)
    }

    // ─────────────────────────────────────────────────────────────
    // Subscription count
    // ─────────────────────────────────────────────────────────────

    /// Returns the current number of active subscriptions.
    pub fn get_active_count(env: Env) -> u64 {
        subscription_count::get_active_count(&env)
    }

    // ─────────────────────────────────────────────────────────────
    // Subscriber index
    // ─────────────────────────────────────────────────────────────

    /// Returns the total number of unique subscribers ever recorded (append-only count).
    pub fn get_subscriber_count(env: Env) -> u64 {
        subscription_count::get_subscriber_index_size(&env)
    }

    /// Returns the subscriber address at the given index slot, or `None` if out of range.
    pub fn get_subscriber_at(env: Env, index: u64) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::SubscriberIndex(index))
    }

    /// Returns a page of subscriber addresses starting at `offset`, capped at 50 per call.
    /// Returns an empty Vec when `offset >= count` or `limit == 0`.
    pub fn get_subscriber_page(env: Env, offset: u64, limit: u32) -> Vec<Address> {
        let count = subscription_count::get_subscriber_index_size(&env);
        let cap: u32 = if limit > 50 { 50 } else { limit };
        let mut result = Vec::new(&env);
        if offset >= count || cap == 0 {
            return result;
        }
        let mut i = offset;
        let end = offset + cap as u64;
        while i < end && i < count {
            if let Some(addr) = env
                .storage()
                .persistent()
                .get(&DataKey::SubscriberIndex(i))
            {
                result.push_back(addr);
            }
            i += 1;
        }
        result
    }

    // ─────────────────────────────────────────────────────────────
    // Merchant revenue
    // ─────────────────────────────────────────────────────────────

    /// Returns the total amount charged to a merchant's subscribers
    /// (sum of all successful `charge()` and `pay_per_use()` calls).
    pub fn get_merchant_revenue(env: Env, merchant: Address) -> i128 {
        merchant_stats::get_merchant_revenue(&env, &merchant)
    }

    /// Returns per-charge revenue entries for the merchant (up to `days` most recent).
    /// Oldest -> newest. Returns an empty Vec when no history has been recorded or after clearing.
    pub fn get_merchant_revenue_history(env: Env, merchant: Address, days: u32) -> Vec<i128> {
        merchant_stats::get_merchant_revenue_history(&env, &merchant, days)
    }

    /// Clears the merchant's revenue history Vec from persistent storage.
    /// Only the contract admin can call this. Idempotent — safe to call when no history exists.
    /// Does not affect the cumulative revenue total.
    pub fn clear_merchant_revenue_history(env: Env, merchant: Address) {
        admin::require_admin(&env);
        merchant_stats::clear_revenue_history(&env, &merchant);
        events::publish_merchant_history_cleared(&env, &merchant);
    }

    // ─────────────────────────────────────────────────────────────
    // Daily spending limits
    // ─────────────────────────────────────────────────────────────

    /// Sets a daily spending cap for `pay_per_use()` for the calling user.
    /// Stored in temporary storage; resets automatically after ~1 day.
    pub fn set_daily_limit(env: Env, user: Address, limit: i128) {
        user.require_auth();
        assert!(limit > 0, "limit must be positive");
        spending_limit::set_daily_limit(&env, &user, limit);
    }

    /// Returns the current daily spending limit for the caller, or `None` if unset.
    pub fn get_daily_limit(env: Env, user: Address) -> Option<i128> {
        spending_limit::get_daily_limit(&env, &user)
    }

    /// Returns the amount spent so far today via `pay_per_use()` for the caller.
    pub fn get_daily_spent(env: Env, user: Address) -> i128 {
        spending_limit::get_daily_spent(&env, &user)
    }

    // ─────────────────────────────────────────────
    // Referral tracking
    // ─────────────────────────────────────────────────────────────

    /// Returns the referrer address for a given subscriber, or `None`.
    pub fn get_referrer(env: Env, user: Address) -> Option<Address> {
        referral::get_referrer(&env, &user)
    }

    // ─────────────────────────────────────────────────────────────
    // State migration
    // ─────────────────────────────────────────────────────────────

    /// Migrates contract storage to the latest schema version.
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn migrate(env: Env) {
        migration::migrate(&env);
    }

    /// Returns the current storage schema version.
    pub fn get_schema_version(env: Env) -> u32 {
        migration::get_schema_version(&env)
    }

    // ─────────────────────────────────────────────────────────────
    // Subscription metadata
    // ─────────────────────────────────────────────────────────────

    /// Attaches a short label (e.g. plan name) to the caller's subscription.
    pub fn set_metadata(env: Env, user: Address, label: String) {
        user.require_auth();
        subscription_metadata::set_metadata(&env, &user, label);
    }

    /// Returns the metadata label for a subscriber, or `None` if not set.
    pub fn get_metadata(env: Env, user: Address) -> Option<String> {
        subscription_metadata::get_metadata(&env, &user)
    }

    // ─────────────────────────────────────────────────────────────
    // Charge history
    // ─────────────────────────────────────────────────────────────

    /// Returns the last (up to 12) charge timestamps for a subscriber,
    /// ordered oldest → newest.
    pub fn get_charge_history(env: Env, user: Address) -> Vec<u64> {
        subscription_history::get_charge_history(&env, &user)
    }

    // ─────────────────────────────────────────────────────────────
    // Admin setup
    // ─────────────────────────────────────────────────────────────

    /// Sets the contract admin. Can only be called once; subsequent calls panic.
    pub fn set_initial_admin(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("admin already set");
        }
        storage::set_admin(&env, &admin);
    }

    // ─────────────────────────────────────────────────────────────
    // Contract pause
    // ─────────────────────────────────────────────────────────────

    /// Pauses the contract. Only the admin can call this.
    pub fn pause_contract(env: Env) {
        admin::require_admin(&env);
        storage::set_contract_paused(&env, true);
    }

    /// Unpauses the contract. Only the admin can call this.
    pub fn unpause_contract(env: Env) {
        admin::require_admin(&env);
        storage::set_contract_paused(&env, false);
    }

    // ─────────────────────────────────────────────────────────────
    // Health check
    // ─────────────────────────────────────────────────────────────

    /// Returns a snapshot of contract health. Safe to call at any time — no auth required, no storage writes.
    pub fn contract_health_check(env: Env) -> HealthReport {
        let contract_paused = storage::is_contract_paused(&env);
        let token_configured = storage::get_token(&env).is_some();
        let admin_configured = storage::get_admin_optional(&env).is_some();
        let instance_ttl_ledgers = env.storage().instance().get_ttl();
        let active_subscription_count = subscription_count::get_active_count(&env);
        let schema_version = migration::get_schema_version(&env);

        // Healthy when not paused, fully configured, and at least 1 day of TTL remaining (17_280 ledgers at ~5 s/ledger)
        let is_healthy = !contract_paused
            && token_configured
            && admin_configured
            && instance_ttl_ledgers > 17_280;

        HealthReport {
            is_healthy,
            contract_paused,
            token_configured,
            admin_configured,
            instance_ttl_ledgers,
            active_subscription_count,
            schema_version,
        }
    }
}

fn extend_subscription_ttl(env: &Env, user: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Subscription(user.clone()),
        SUBSCRIPTION_TTL_LEDGERS,
        SUBSCRIPTION_TTL_LEDGERS,
    );
}
