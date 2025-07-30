use crate::rpchandler::rpc_types::{RpcTypes, SubscriptionType};
use crate::transactionTypes::*;
use alloy::signers::k256::ecdsa::SigningKey;
use alloy::signers::local::LocalSigner;
use alloy::{
    network::{EthereumWallet, NetworkWallet},
    primitives::Address,
    rpc::types::Log,
};
use dashmap::DashMap;
use std::{collections::BTreeMap, default, error::Error};
use tokio::{
    sync::{mpsc, oneshot},
    time,
};

pub struct RelayerHandler {
    RpcCommand_sender: DashMap<usize, mpsc::Sender<SubscriptionType>>,
    log_receiver: mpsc::Receiver<Log>,
    command_receiver: mpsc::Receiver<RelayerCommand>,
    relayers: DashMap<Address, LocalSigner<SigningKey>>,
    actions: DashMap<String, RawTransaction>,
    user_logs: Arc<DashMap<Address, BTreeMap<UserUpdates>>>,
}

pub struct UserUpdates {
    Message: String,
    tx: String,
    time: time::Instant,
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
            RelayerCommand::Register { user } => {}
            RelayerCommand::GetLogs { user } => {}
            RelayerCommand::UnRegister { user } => {}
            RelayerCommand::DefineRelayerAction {
                sub_id,
                chainid,
                target_address,
                ABI,
                function_name,
                Params,
            } => {}
            RelayerCommand::Revoke_Relayer_Action { user, sub_id } => {}
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
                let res = wallet.sign_request(tran).await?;
            }
        }

        Ok(())
    }
}

pub enum RelayerCommand {
    Register {
        user: String,
    },
    UnRegister {
        user: String,
    },
    GetLogs {
        user: String,
    },
    DefineRelayerAction {
        sub_id: String,
        chainid: int,
        target_address: String,
        ABI: String,
        function_name: String,
        Params: Vec<String>,
    },

    Revoke_Relayer_Action {
        user: String,
        sub_id: String,
    },
}
