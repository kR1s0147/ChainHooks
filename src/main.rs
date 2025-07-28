#![allow(warnings)]
use crate::rpchandler::relayer::RelayerCommand;
use crate::rpchandler::rpc_types::SubscriptionType;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use alloy::primitives::Address;
use dashmap::DashMap;
use eyre::Ok;
use rand::TryRngCore;
use tokio::net::unix::pipe::Sender;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tonic::{Request, Response, Status, transport::Server};
mod rpchandler;
use rpchandler::*;
pub mod chainhooks {
    tonic::include_proto!("chainhooks");
}

use chainhooks::chain_hooks_server::{ChainHooks, ChainHooksServer};
use chainhooks::*;

#[derive(Default)]
pub struct RelayerService {
    user_nonce: DashMap<Address, UserNonce>,
    RelayerCommand_sender: Sender<RelayerCommand>,
    RpcCommand_Sender: Sender,
}

struct UserNonce {
    nonce: String,
    time: Instant,
}

//     rpc GetNonce(GetNonceRequest) returns (GetNonceResponse);
//     rpc Register(UserAuthRequest) returns (UserRegistrationResponse);
//     rpc GetRelayer(UserAuthRequest) returns (RelayerInfo);
//     rpc UnRegister(UserAuthRequest) returns (UserRegistrationResponse);
//     rpc GetLogs(GetUserLogsRequest) returns (UserLogs);
//     rpc Subscribe(SubscriptionRequest) returns (SubscriptionResponse);
//     rpc UnSubscribe(UnsubscribeRequest) returns (google.protobuf.Empty);

#[tonic::async_trait]
impl ChainHooks for RelayerService {
    // return nonce
    async fn get_nonce(
        &mut self,
        userRequest: Request<GetNonceRequest>,
    ) -> Response<GetNonceResponse> {
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
    async fn register(
        &mut self,
        userRequest: Request<UserAuthRequest>,
    ) -> Response<UserRegistrationResponse> {
        let req = userRequest.into_inner();
        let addr = Address::from_str(req.address.as_str());
        let user_addr = match addr {
            Ok(user) => user,
            Err(_) => {
                let res = UserRegistrationResponse {
                    success: false,
                    message: String::from("Invalid Address"),
                    user_id: String::from("NA"),
                };
                return Response::new(res);
            }
        };

        let relayer_cmd = RelayerCommand::Register { user: user_addr };
        let res = self.RelayerCommand_sender.send(relayer_cmd).await?;
    }

    async fn un_register(
        &mut self,
        userRequest: Request<UserAuthRequest>,
    ) -> Response<UserRegistrationResponse> {
    }

    async fn get_relayer(
        &mut self,
        userRequest: Request<UserAuthRequest>,
    ) -> Response<RelayerInfo> {
        // Relayer Info (pun key)
        todo!()
    }
    async fn get_logs(&self, userRequest: Request<GetUserLogsRequest>) -> Response<UserLogs> {
        todo!()
    }
    async fn subscribe(
        &mut self,
        userRequest: Request<SubscriptionRequest>,
    ) -> Response<SubscriptionResponse> {
        todo!()
    }
    async fn un_subscribe(&mut self, userRequest: Request<UnsubscribeRequest>) -> Response<()> {
        todo!()
    }
}

#[tokio::main]
async fn main() {}
