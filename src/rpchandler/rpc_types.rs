use std::ops::Add;
use std::sync::Arc;

use crate::rpchandler::relayer::UserUpdates;
use alloy::primitives::{Address, ChainId};
use alloy::rpc::types::{Filter, Log, TransactionRequest};
use alloy::signers::k256::ecdsa::SigningKey;
use alloy::signers::k256::sha2::digest::const_oid::Arc;
use alloy::signers::local::LocalSigner;
use dashmap::DashMap;
use serde::{Deserialize, Serialize, de};
use std::collections::BTreeMap;
use std::time;
use thiserror::Error;
use tokio::sync::{Mutex, mpsc};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SubscriptionType {
    Subscription {
        user: Address,
        chainid: usize,
        address: Vec<Address>,
        event_signature: Vec<String>,
    },
    Transaction {
        user: Address,
        signer: LocalSigner<SigningKey>,
        tx: TransactionRequest,
        db: Arc<Mutex<DashMap<Address, BTreeMap<time::Instant, UserUpdates>>>>,
    },
    Revoke_Sub {
        user: Address,
        subs: String,
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
    Logs {
        logs: Vec<UserUpdates>,
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
