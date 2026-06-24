use soroban_sdk::{Address, Env, Symbol};

use crate::Subscription;

pub fn publish_subscribed(env: &Env, user: &Address, sub: &Subscription) {
    env.events().publish(
        (Symbol::new(env, "subscribed"), user.clone()),
        (sub.merchant.clone(), sub.amount, sub.interval),
    );
}

pub fn publish_charged(env: &Env, user: &Address, sub: &Subscription, charged_at: u64) {
    env.events().publish(
        (Symbol::new(env, "charged"), user.clone()),
        (sub.merchant.clone(), sub.amount, charged_at),
    );
}

pub fn publish_pay_per_use(env: &Env, user: &Address, merchant: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "pay_per_use"), user.clone()),
        (merchant.clone(), amount),
    );
}

pub fn publish_cancelled(env: &Env, user: &Address) {
    env.events()
        .publish((Symbol::new(env, "cancelled"), user.clone()), ());
}

pub fn publish_min_interval_updated(env: &Env, seconds: u64) {
    env.events()
        .publish((Symbol::new(env, "min_interval"),), seconds);
}

pub fn publish_merchant_history_cleared(env: &Env, merchant: &Address) {
    env.events()
        .publish((Symbol::new(env, "merch_hist_cleared"),), merchant.clone());
}
