use alloy::primitives::{Address, ChainId};
use alloy::rpc::types::Filter;
use thiserror::Error;

pub struct IncomingSubscription {
    sub: Subscription,
    signature: String,
}

pub enum SubscriptionType {
    Subscription {
        user: Address,
        chainid: usize,
        address: Vec<Address>,
        event_signature: String,
        topics: Option<Vec<String>>,
    },
}

pub enum RpcTypes {}

/// Errors for RPC operations           

#[derive(Error, Debug)]
enum RpcError {
    #[error("Chain is Not Supported")]
    ChainNotSupported,
    #[error("Chain has no URL")]
    ChainHasNoRpcURL,
}
