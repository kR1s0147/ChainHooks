use alloy::primitives::{Address, ChainId};
use alloy::rpc::types::Filter;
use serde::{Deserialize, Serialize, de};
use thiserror::Error;
use tokio::sync::mpsc;

pub struct IncomingSubscription {
    sub: SubscriptionType,
    signature: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SubscriptionType {
    Subscription {
        user: Address,
        chainid: usize,
        address: Vec<Address>,
        event_signature: Vec<String>,
    },
}

pub enum RpcTypes {
    UserLog { user: Address, log: String },
}

/// Errors for RPC operations           

#[derive(Error, Debug)]
pub enum RpcTypeError {
    #[error("Chain is Not Supported")]
    ChainNotSupported,
    #[error("Chain has no URL")]
    ChainHasNoRpcURL,
    #[error("User subscription is not found")]
    NoSubscriptionFound,
}

#[derive(Clone)]
pub struct ChainState {
    pub active: bool,
    pub chain_url: String,
    pub channel: Option<mpsc::Sender<SubscriptionType>>,
}
