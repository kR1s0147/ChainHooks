use alloy::primitives::{Address, ChainId};
use alloy::rpc::types::{Filter, Log, TransactionRequest};
use alloy::signers::k256::ecdsa::SigningKey;
use alloy::signers::local::LocalSigner;
use serde::{Deserialize, Serialize, de};
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SubscriptionType {
    Subscription {
        user: Address,
        chainid: usize,
        address: Vec<Address>,
        event_signature: Vec<String>,
    },
    Revoke_User {
        user: Address,
    },
    Transaction {
        signer: LocalSigner<SigningKey>,
        tx: TransactionRequest,
    },
}

pub enum RpcTypes {
    UserLog {
        user: Address,
        sub_id: String,
        log: Log,
    },
    Response {
        success: bool,
        message: String,
    },
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
    #[error("Error while subscription")]
    SubscriptionError,
}

#[derive(Clone)]
pub struct ChainState {
    pub active: bool,
    pub chain_url: String,
    pub channel: Option<mpsc::Sender<SubscriptionType>>,
}
