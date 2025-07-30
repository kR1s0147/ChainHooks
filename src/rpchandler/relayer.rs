use crate::rpchandler::rpc_types::{RpcTypes, SubscriptionType};
use crate::transactionTypes::*;
use alloy::network::TransactionBuilder;
use alloy::signers::k256::ecdsa::SigningKey;
use alloy::signers::local::LocalSigner;
use alloy::{
    network::{EthereumWallet, NetworkWallet},
    primitives::Address,
    rpc::types::Log,
};
use dashmap::DashMap;
use std::ops::Add;
use std::str::FromStr;
use std::sync::Arc;
use std::{collections::BTreeMap, default, error::Error};
use tokio::{
    sync::{Mutex, mpsc, oneshot},
    time,
};

pub struct RelayerHandler {
    RpcCommand_sender: DashMap<usize, mpsc::Sender<SubscriptionType>>,
    log_receiver: mpsc::Receiver<Log>,
    command_receiver: mpsc::Receiver<RelayerCommand>,
    relayers: DashMap<Address, UserInfo>,
    actions: DashMap<String, RawTransaction>,
    user_logs: Arc<Mutex<DashMap<Address, BTreeMap<time::Instant, UserUpdates>>>>,
}

pub struct UserInfo {
    signer: LocalSigner<SigningKey>,
    subs: Vec<String>,
}

pub struct UserUpdates {
    Message: String,
    tx: String,
}

impl RelayerHandler {
    fn new_handler(
        log_receiver: mpsc::Receiver<RpcTypes::UserLog>,
        command_receiver: mpsc::Receiver<RelayerCommand>,
    ) -> Self {
        RelayerHandler {
            RpcCommand_sender: Default::default(),
            log_receiver,
            command_receiver,
            relayers: Default::default(),
            actions: Default::default(),
            user_logs: Default::default(),
        }
    }

    fn new_relayer(&mut self, address: String) -> Result<Address, Box<dyn Error>> {
        let addr = match Address::try_from(address.as_bytes()) {
            Ok(add) => add,
            Err(e) => return Err(Box::new(RelayerError::InvalidAddress)),
        };
        match self.relayers.get(&addr) {
            Some(_) => return Err(Box::new(RelayerError::AlreadyRegistered)),
            None => {}
        };
        let signer = LocalSigner::random();

        self.relayers.insert(addr, signer);

        Ok(signer.address())
    }

    async fn register(&mut self, user: Address) -> Result<Address, Box<dyn Error + Send + Sync>> {
        self.new_relayer(address)
    }

    async fn run(&mut self) -> Result<(), Box<dyn Error + send + sync>> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some((command , res_receiver)) = command_receiver.recv() => {
                        self.handle_command(command,res_receiver).await;
                    }
                    (log, response_receiver) = log_receiver.recv() => {
                        self.handle_log(log,response_receiver).await

                    }
                }
            }
        })
    }

    async fn handle_command(
        &mut self,
        command: RelayerCommand,
        res_receiver: oneshot::Sender<RpcTypes::Response>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match command {
            RelayerCommand::Register { user } => self.register(user),
            RelayerCommand::GetLogs { user, time } => {
                let addr: Address;
                if let Some(addr_) = Address::from_str(user) {
                    addr = addr_;
                }
                let logs = self.user_logs.lock().await;
                if let Some(map) = logs.get(addr) {
                    let res_logs = Vec::new();
                    for (i, logs) in map.range(time..) {
                        res_logs.push(logs.clone());
                    }
                    res_receiver.send(RpcTypes::Logs { logs: res_logs }).await
                }
            }
            RelayerCommand::DefineRelayerAction {
                user,
                sub_id,
                chainid,
                target_address,
                ABI,
                function_name,
                function_signature,
                Params,
            } => {
                let raw_tran = RawTransaction::new(
                    chainid,
                    target_address,
                    ABI,
                    function_name,
                    function_signature,
                    params,
                );
                if let Some(addr) = Address::from_str(user) {
                    self.actions.insert(sub_id, raw_tran);
                    if let Some(userinfo) = self.relayers.get_mut(addr) {
                        userinfo.subs.push(sub_id);
                    }
                    res_receiver
                        .send(RpcTypes::Response {
                            success: true,
                            message: "SuccessFully added",
                        })
                        .await;
                }
            }
            RelayerCommand::Revoke_Subscription { user, sub_id } => {
                if let Some(addr) = Address::from_str(user) {
                    if let Some(userinfo) = self.relayers.get_mut(addr) {
                        if let Some(tran) = self.actions.get(sub_id) {
                            let send = SubscriptionType::Revoke_Sub {
                                user: user,
                                subs: sub_id,
                            };
                            self.actions.remove(sub_id);
                            userinfo.subs.retain(|s| s != sub_id);
                            let chainid = tran.chain_id;
                            if let Some(ch) = self.RpcCommand_sender.get_mut(&chainid) {
                                ch.send(send).await;
                            }

                            res_receiver.send(RpcTypes::Response {
                                success: true,
                                message: "Revoked the Subscription",
                            })
                        }
                    }
                }
            }
        }
    }

    async fn handle_log(
        &mut self,
        log: RpcTypes,
        response_receiver: Receiver<RelayerCommand>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let addr;
        let subid;
        let Userlog;
        match log {
            RpcTypes::UserLog { user, sub_id, log } => {
                addr = user;
                subid = sub_id;
                Userlog = log;
            }
            _ => {}
        }
        let transaction: RawTransaction;
        if let Some(raw_tran) = self.actions.get(sub_id) {
            transaction = raw_tran.clone();
        }

        if let Some(wallet) = self.relayers.get_mut(&addr) {
            if let Ok(tran) = transaction.build_transaction(log) {
                let s = wallet.clone();
                tran.with_from(s.address())
                    .with_chain_id(transaction.chain_id);
                let res = SubscriptionType::Transaction {
                    signer: s,
                    tx: tran,
                };
                if let Some(ch) = self.RpcCommand_sender.get_mut(&transaction.chain_id) {
                    ch.send(res).await
                }
            }
        }
        Ok(())
    }
}

pub enum RelayerCommand {
    Register {
        user: String,
    },
    GetLogs {
        user: String,
        time: time::Instant,
    },
    DefineRelayerAction {
        user: String,
        sub_id: String,
        chainid: int,
        target_address: String,
        ABI: String,
        function_name: String,
        function_signature: String,
        Params: Vec<String>,
    },
    Revoke_Subscription {
        user: String,
        sub_id: String,
    },
}
