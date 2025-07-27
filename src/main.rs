#![allow(warnings)]
use crate::rpchandler::rpc_types::SubscriptionType;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use alloy::primitives::Address;
use eyre::Ok;
use rand::TryRngCore;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tonic::{Request, Response, Status, transport::Server};
mod rpchandler;

pub mod chainhooks {
    tonic::include_proto!("chainhooks");
}

use chainhooks::chain_hooks_server::{ChainHooks, ChainHooksServer};
use chainhooks::*;

#[derive(Default)]
pub struct RelayerService {
    user_nonce: Arc<Mutex<HashMap<Address, UserNonce>>>,
}

struct UserNonce {
    nonce: String,
    time: Instant,
}

#[tonic::async_trait]
impl ChainHooks for RelayerService {
    // return nonce
    async fn get_nonce(&self, userRequest: Request<GetNonceRequest>) -> Response<GetNonceResponse> {
        let req = userRequest.into_inner();
        let user = req.address;
        let user_addr = Address::from_str(user);
        let mut random_bytes = [0u8; 16];
        rand::rngs::OsRng.try_fill_bytes(&mut random_bytes);

        let nonce = UserNonce {
            nonce: hex::encode(random_bytes),
            time: Instant::now(),
        };

        let users_nonce = self.user_nonce.lock().await;
        users_nonce.insert(user_addr, nonce);

        Ok(Response::new(GetNonceResponse { nonce }))
    }
}

#[tokio::main]
async fn main() {}
