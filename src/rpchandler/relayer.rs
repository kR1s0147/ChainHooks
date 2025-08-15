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

pub struct UserInfo {
    pub signer: LocalSigner<SigningKey>,
    pub subs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct UserUpdates {
    pub Message: String,
    pub tx: String,
}

pub struct RelayerHandler {
    RpcCommand_sender: DashMap<usize, mpsc::Sender<SubscriptionType>>,
    log_receiver: Arc<mpsc::Receiver<Log>>,
    command_receiver: Arc<mpsc::Receiver<(RelayerCommand, oneshot::Sender<RpcTypes>)>>,
    relayers: DashMap<Address, UserInfo>,
    actions: DashMap<String, RawTransaction>,
    user_logs: Arc<DashMap<Address, BTreeMap<time::Instant, UserUpdates>>>,
}

impl RelayerHandler {
    pub fn new_handler(
        log_receiver: mpsc::Receiver<RpcTypes>,
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

    pub async fn register(&mut self, user: String) -> Result<Address, Box<dyn Error>> {
        self.new_relayer(user)
    }

    pub async fn run(self) -> Result<(), Box<dyn Error>> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some((command , res_receiver)) = self.command_receiver.recv() => {
                        self.handle_command(command,res_receiver).await;
                    }
                    log = self.log_receiver.recv() => {
                        self.handle_log(log).await?;

                    }
                }
            }
        });
        Ok(())
    }

    async fn handle_command(
        &mut self,
        command: RelayerCommand,
        res_receiver: oneshot::Sender<RpcTypes>,
    ) -> Result<(), Box<dyn Error>> {
        match command {
            RelayerCommand::Register { user } => {
                let addr = match self.new_relayer(user) {
                    Ok(addr) => addr,
                    Err(e) => {
                        res_receiver.send(RpcTypes::Response {
                            success: false,
                            message: e.to_string(),
                        });
                        return Ok(());
                    }
                };
                res_receiver.send(RpcTypes::Response {
                    success: true,
                    message: addr.to_string(),
                });
            }

            RelayerCommand::GetLogs { user, time } => {
                if let Ok(addr) = Address::from_str(user.as_str()) {
                    let logs = self.user_logs.clone();
                    if let Some(map) = logs.get(&addr) {
                        let mut res_logs = Vec::new();
                        for (i, logs) in map.range(time..) {
                            res_logs.push(logs.clone());
                        }
                        res_receiver.send(RpcTypes::Logs { logs: res_logs });
                    }
                }
            }

            RelayerCommand::DefineRelayerAction {
                user,
                sub_id,
                chainid,
                target_address,
                ABI,
                function_name,
                Params,
            } => {
                let raw_tran =
                    RawTransaction::new(chainid, target_address, ABI, function_name, Params);
                if let Ok(addr) = Address::from_str(user.as_str()) {
                    self.actions.insert(sub_id.clone(), raw_tran);
                    if let Some(mut userinfo) = self.relayers.get_mut(&addr) {
                        userinfo.subs.push(sub_id);
                    }
                    res_receiver.send(RpcTypes::Response {
                        success: true,
                        message: "SuccessFully added".to_string(),
                    });
                }
            }
            RelayerCommand::Revoke_Subscription { user, sub_id } => {
                if let Ok(addr) = Address::from_str(user.as_str()) {
                    if let Some(mut userinfo) = self.relayers.get_mut(&addr) {
                        if let Some(tran) = self.actions.get(&sub_id) {
                            let send = SubscriptionType::Revoke_Sub {
                                user: addr,
                                subs: sub_id.clone(),
                            };
                            self.actions.remove(&sub_id);
                            userinfo.subs.retain(|s| s != &sub_id);
                            let chainid = tran.chain_id;
                            if let Some(ch) = self.RpcCommand_sender.get_mut(&chainid) {
                                ch.send(send).await?;
                            }
                            res_receiver.send(RpcTypes::Response {
                                success: true,
                                message: "Revoked the Subscription".to_string(),
                            });
                        }
                    }
                }
            }

            RelayerCommand::Get_RalyerInfo { user } => {
                if let Some(addr) = Address::from_str(user.as_str()).ok() {
                    if let Some(info) = self.relayers.get(&addr) {
                        let signer = info.signer.address();
                        res_receiver.send(RpcTypes::Response {
                            success: true,
                            message: signer.to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_log(&mut self, log: RpcTypes) -> Result<(), Box<dyn Error>> {
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
        if let Some(raw_tran) = self.actions.get(&subid) {
            transaction = raw_tran.clone();
        }

        if let Some(wallet) = self.relayers.get_mut(&addr) {
            if let Ok(tran) = transaction.build_transaction(log) {
                let s = wallet.signer.clone();
                let db = self.user_logs;
                tran.with_from(s.address())
                    .with_chain_id(transaction.chain_id as u64);

                let res = SubscriptionType::Transaction {
                    user: addr.clone(),
                    signer: s,
                    tx: tran,
                    db: db.clone(),
                };

                if let Some(ch) = self.RpcCommand_sender.get_mut(&transaction.chain_id) {
                    let (_, sender) = oneshot::channel::<RpcTypes>();
                    ch.send((res, sender)).await
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
        chainid: usize,
        target_address: String,
        ABI: String,
        function_name: String,
        Params: Vec<(usize, String)>,
    },
    Revoke_Subscription {
        user: String,
        sub_id: String,
    },
    Get_RalyerInfo {
        user: String,
    },
}
