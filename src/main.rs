#![allow(warnings)]
use crate::rpchandler::relayer::RelayerCommand;
use crate::rpchandler::rpc_types::{RpcTypes, SubscriptionType};
use std::error::Error;
use std::ops::Add;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy::dyn_abi::ErrorExt;
use alloy::primitives::{Address, Signature};

use dashmap::DashMap;

use rand::TryRngCore;

use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{self, Instant};
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
    RelayerCommand_sender: mpsc::Sender<(RelayerCommand, oneshot::Sender<RpcTypes>)>,
    RpcHandler: RPChandler,
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
    ) -> Result<Response<GetNonceResponse>, Status> {
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
    ) -> Result<Response<UserRegistrationResponse>, Status> {
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
        let res = self.RelayerCommand_sender.send(relayer_cmd);
    }

    async fn get_relayer(
        &mut self,
        userRequest: Request<UserAuthRequest>,
    ) -> Result<Response<RelayerInfo>, Status> {
        // Relayer Info (pun key)
        /// Send command to Relayer
        /// receive the infomation
        let user = userRequest.into_inner().address;
        let req = RelayerCommand::Get_RalyerInfo { user };
        let (rx, tx) = oneshot::channel::<RpcTypes>();
        self.RelayerCommand_sender.send((req, rx));
        let res = tx.await?;
        match res {
            RpcTypes::Response { success, message } => {
                if success {
                    return Response::new(RelayerInfo {
                        owner_address: user,
                        relayer_public_key: message,
                    });
                }
            }
            _ => {}
        }
        return Err(Status::not_found("User is not registered"));
    }
    async fn get_logs(
        &self,
        userRequest: Request<GetUserLogsRequest>,
    ) -> Result<Response<UserLogs>, Status> {
        let user = userRequest.into_inner().address;
        let time = userRequest.into_inner().start_time.unwrap();

        let req = RelayerCommand::GetLogs {
            user,
            time: time::Instant::checked_add(&self, Duration::from_secs(time.seconds)).unwrap(),
        };

        let (tx, rx) = oneshot::channel::<RpcTypes>();
        self.RelayerCommand_sender.send((req, tx));
        let res = rx.await.unwrap();
        match res {
            RpcTypes::Logs { logs } => {
                return Ok(Response::new(UserLogs {
                    address: user,
                    logs: logs.to_string(),
                }));
            }
            _ => {}
        }
        Err(Status::not_found("Logs not Found"))
    }
    async fn subscribe(
        &mut self,
        userRequest: Request<SubscriptionRequest>,
    ) -> Result<Response<SubscriptionResponse>, Status> {
        let req = userRequest.into_inner();
        let user = Address::from_str(&req.address).unwrap();
        let sub = req.details.unwrap();
        let rpc_command = SubscriptionType::Subscription {
            user,
            chainid: sub.chain_id,
            address: sub.target_address,
            event_signature: sub.event_signature,
        };

        let (tx, rx) = oneshot::channel::<RpcTypes>();

        let ch = self
            .RpcHandler
            .chain_state
            .get(sub.chain_id)
            .unwrap()
            .channel
            .unwrap();

        ch.send((rpc_command, tx)).await;

        let res = rx.await.unwrap();

        match res {
            RpcTypes::Response { success, message } => {
                if success {
                    let action = req.action.unwrap();
                    let relayer_command = RelayerCommand::DefineRelayerAction {
                        user,
                        sub_id: message,
                        chainid: action.chain_id,
                        target_address: action.target_address,
                        ABI: action.abi,
                        function_name: action.function_name,
                        Params: action.params,
                    };
                    let (tx, rx) = oneshot::channel::<RpcTypes>();
                    self.RelayerCommand_sender.send((relayer_command, tx)).await;

                    let res = rx.await.unwrap();
                    match res {
                        RpcTypes::Response { success, message } => {
                            if success {
                                return Ok(Response::new(SubscriptionResponse {
                                    subscription_id: message,
                                    success: true,
                                    message: String::from("Subscription Created"),
                                    created_at: time::Instant::now(),
                                }));
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Err(Status::internal("error while subcription"))
    }
    async fn un_subscribe(
        &mut self,
        userRequest: Request<UnsubscribeRequest>,
    ) -> Result<Response<bool>, Status> {
        let req = userRequest.into_inner();
        let user = Address::from_str(&req.address).unwrap();
        let relayer_command = RelayerCommand::Revoke_Subscription {
            user: req.address,
            sub_id: req.subscription_id,
        };

        let (tx, rx) = oneshot::channel::<RpcTypes>();

        self.RelayerCommand_sender.send((relayer_command, tx)).await;
        let res = rx.await?;
        match res {
            RpcTypes::Response { success, message } => {
                if success {
                    return Ok(Response::new(true));
                }
            }
            _ => {}
        }
    }
}

pub struct UserTx {
    user: Address,
    Signature: String,
}

impl UserTx {
    fn new(user: String, signature: String) -> Self {
        if let Ok(addr) = Address::from_str(&user) {
            UserTx {
                user: addr,
                Signature: signature,
            }
        }
    }

    async fn VerifyUser(&self, nonce: String) -> Result<bool, Box<dyn Error>> {
        if let Ok(sign) = Signature::from_str(&self.Signature) {
            if let Ok(addr) = sign.recover_address_from_msg(nonce) {
                if addr == self.user {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[tokio::main]
async fn main() {}
