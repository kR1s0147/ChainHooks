#![allow(warnings)]
use crate::rpchandler::relayer::{RelayerCommand, RelayerHandler};
use crate::rpchandler::rpc_types::{RpcTypes, SubscriptionType};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt::format;
use std::str::FromStr;
use std::sync::Arc;

use alloy::dyn_abi::ErrorExt;
use alloy::primitives::{Address, Signature};

use alloy::rpc::types::Log;
use dashmap::DashMap;
use rand::TryRngCore;

use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{self, Duration, Instant};
use tonic::{Request, Response, Status, transport::Server};
mod rpchandler;
use rpchandler::*;
pub mod chainhooks {
    tonic::include_proto!("chainhooks");
}

use chainhooks::chain_hooks_server::{ChainHooks, ChainHooksServer};
use chainhooks::*;

use dotenv::dotenv;

pub struct RelayerService {
    user_nonce: DashMap<Address, UserNonce>,
    RelayerCommand_sender: mpsc::Sender<(RelayerCommand, oneshot::Sender<RpcTypes>)>,
    RpcHandler: Mutex<RPChandler>,
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
        &self,
        userRequest: Request<GetNonceRequest>,
    ) -> Result<Response<GetNonceResponse>, Status> {
        let req = userRequest.into_inner();
        let user = req.address;
        let user_addr = Address::from_str(&user).unwrap();
        let mut random_bytes = [0u8; 16];
        rand::rngs::OsRng.try_fill_bytes(&mut random_bytes);
        let n = hex::encode(random_bytes);
        let nonce = UserNonce {
            nonce: n.clone(),
            time: Instant::now(),
        };

        let users_nonce = &self.user_nonce;
        users_nonce.insert(user_addr, nonce);

        Ok(Response::new(GetNonceResponse { nonce: n }))
    }
    async fn register(
        &self,
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
                return Ok(Response::new(res));
            }
        };
        let usertx = UserTx::new(req.address, req.signature).unwrap();
        if let Some(n) = self.user_nonce.get(&user_addr) {
            if !usertx
                .VerifyUser(n.nonce.clone())
                .await
                .expect("User verification failed")
            {
                return Err(Status::permission_denied("Not Authenticated"));
            }
        }
        let relayer_cmd = RelayerCommand::Register {
            user: user_addr.to_string(),
        };
        let (tx, _rx) = oneshot::channel::<RpcTypes>();
        let res = self.RelayerCommand_sender.send((relayer_cmd, tx)).await;
        Ok(Response::new(UserRegistrationResponse {
            success: res.is_ok(),
            message: if res.is_ok() {
                String::from("User Registered Successfully")
            } else {
                String::from("Failed to Register User")
            },
            user_id: user_addr.to_string(),
        }))
    }

    async fn get_relayer(
        &self,
        userRequest: Request<UserAuthRequest>,
    ) -> Result<Response<RelayerInfo>, Status> {
        // Relayer Info (pun key)
        /// Send command to Relayer
        /// receive the infomation
        let req = userRequest.into_inner();
        let user = req.address;
        let addr = Address::from_str(&user).unwrap();
        let usertx = UserTx::new(user.clone(), req.signature).unwrap();
        if let Some(n) = self.user_nonce.get(&addr) {
            if !usertx
                .VerifyUser(n.nonce.clone())
                .await
                .expect("User verification failed")
            {
                return Err(Status::permission_denied("Not Authenticated"));
            }
        }

        let req = RelayerCommand::Get_RalyerInfo { user: user.clone() };
        let (rx, tx) = oneshot::channel::<RpcTypes>();
        self.RelayerCommand_sender.send((req, rx));
        let res = tx.await.unwrap();
        match res {
            RpcTypes::Response { success, message } => {
                if success {
                    return Ok(Response::new(RelayerInfo {
                        owner_address: user,
                        relayer_public_key: message,
                    }));
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
        let req = userRequest.into_inner();
        let user = req.address;

        let addr = Address::from_str(&user).unwrap();
        let usertx = UserTx::new(user.clone(), req.signature).unwrap();
        if let Some(n) = self.user_nonce.get(&addr) {
            if !usertx
                .VerifyUser(n.nonce.clone())
                .await
                .expect("User verification failed")
            {
                return Err(Status::permission_denied("Not Authenticated"));
            }
        }
        let req = RelayerCommand::GetLogs { user: user.clone() };

        let (tx, rx) = oneshot::channel::<RpcTypes>();
        self.RelayerCommand_sender.send((req, tx));
        let res = rx.await.unwrap();
        match res {
            RpcTypes::Logs { logs } => {
                return Ok(Response::new(UserLogs {
                    address: user,
                    logs: logs.into_iter().map(|log| format!("{:?}", log)).collect(),
                }));
            }
            _ => {}
        }
        Err(Status::not_found("Logs not Found"))
    }
    async fn subscribe(
        &self,
        userRequest: Request<SubscriptionRequest>,
    ) -> Result<Response<SubscriptionResponse>, Status> {
        let req = userRequest.into_inner();
        let user = Address::from_str(&req.address).unwrap();
        let usertx = UserTx::new(user.to_string(), req.signature).unwrap();
        if let Some(n) = self.user_nonce.get(&user) {
            if !usertx
                .VerifyUser(n.nonce.clone())
                .await
                .expect("User verification failed")
            {
                return Err(Status::permission_denied("Not Authenticated"));
            }
        }
        let sub = req.details.unwrap();
        let cid = sub.chain_id as usize;
        let rpc_command = SubscriptionType::Subscription {
            user,
            chainid: cid,
            address: sub.target_address.parse::<Address>().unwrap(),
            event_signature: sub.event_signature,
        };

        let (tx, rx) = oneshot::channel::<RpcTypes>();

        let mut ch = self
            .RpcHandler
            .lock()
            .await
            .chain_state
            .get_mut(&cid)
            .unwrap()
            .channel
            .clone()
            .unwrap();

        ch.send((rpc_command, tx)).await;

        let res = rx.await.unwrap();

        match res {
            RpcTypes::Response { success, message } => {
                if success {
                    let action = req.action.unwrap();
                    let params = action
                        .params
                        .into_iter()
                        .map(|p| (p.pos as usize, p.params))
                        .collect::<Vec<_>>();

                    let relayer_command = RelayerCommand::DefineRelayerAction {
                        user: user.to_string(),
                        sub_id: message,
                        chainid: action.chain_id as usize,
                        target_address: action.target_address,
                        ABI: action.abi,
                        function_name: action.function_name,
                        Params: params,
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
        &self,
        userRequest: Request<UnsubscribeRequest>,
    ) -> Result<Response<()>, Status> {
        let req = userRequest.into_inner();
        let user = Address::from_str(&req.address).unwrap();
        let usertx = UserTx::new(user.to_string(), req.signature).unwrap();
        if let Some(n) = self.user_nonce.get(&user) {
            if !usertx
                .VerifyUser(n.nonce.clone())
                .await
                .expect("User verification failed")
            {
                return Err(Status::permission_denied("Not Authenticated"));
            }
        }
        let relayer_command = RelayerCommand::Revoke_Subscription {
            user: req.address,
            sub_id: req.subscription_id,
        };

        let (tx, rx) = oneshot::channel::<RpcTypes>();

        self.RelayerCommand_sender.send((relayer_command, tx)).await;
        let res = rx.await.expect("Failed to receive response");
        match res {
            RpcTypes::Response { success, message } => {
                return Ok(Response::new(()));
            }
            _ => {
                return Ok(Response::new(()));
            }
        }
    }
}

pub struct UserTx {
    user: Address,
    Signature: String,
}

impl UserTx {
    fn new(user: String, signature: String) -> Option<Self> {
        if let Ok(addr) = Address::from_str(&user) {
            return Some(UserTx {
                user: addr,
                Signature: signature,
            });
        }
        None
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

// pub fn Intialize() -> RelayerService {
//     dotenv().ok();

//     let chains = [
//         (1, "ETHEREUM"),
//         (137, "POLYGON"),
//         (42161, "ARBITRUM"),
//         (10, "OPTIMISM"),
//         (11155111, "SEPOLIA"),
//     ];

//     let available_chains: HashMap<_, _> = chains.into_iter().collect();

//     let available_chains = vec![1, 137.10, 42161, 11155111];

//     let mut rpc_handler = RPChandler::new(available_chains);

//     for (chain, name) in chains {
//         if let Some(url) = env::var(name) {
//             rpc_handler.new_chainstate(chain, url);
//         }
//     }

//     let (log_tx, log_rx) = mpsc::channel::<Log>(100);

//     let (relayer_tx, relayer_rx) =
//         mpsc::channel::<(RelayerCommand, oneshot::Sender<RpcTypes>)>(100);

//     let relayer_service = RelayerHandler::new_handler(log_rx, relayer_rx);

//     let Subscriptions = SubscriptionType::Subscription { user: , chainid: (), address: (), event_signature: () }
// }

#[tokio::main]
async fn main() {
    // let handler = Intialize();
}
