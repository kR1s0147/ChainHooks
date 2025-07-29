use alloy::{primitives::Address, rpc::types::Log};
use dashmap::DashMap;
use std::error::Error;
use tokio::{sync::mpsc, time};

use crate::rpchandler::rpc_types::RpcTypes;

pub struct RelayerHandler {
    log_receiver: mpsc::Receiver<Log>,
    command_reciever: mpsc::Receiver<RelayerCommand>,
    relayers: DashMap<Address, EthereumWallet>,
    actions: DashMap<String, RawTransaction>,
    user_logs: Arc<DashMap<Address, Vec<UserLogs>>>,
}

pub struct UserLogs {
    Message: String,
    time: time::Instant,
}

impl RelayerHandler {
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
        let relayer = EthereumWallet::new(signer);

        self.relayers.insert(addr, relayer);

        Ok(signer.address())
    }

    async fn register(&mut self, user: Address) -> Result<Address, Box<dyn Error + Send + Sync>> {
        self.new_relayer(address)
    }

    async fn run(&mut self) -> Result<(), Box<dyn Error + send + sync>> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    command = command_receiver.recv() => {
                        self.handle_command(command)?
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
        let sub_id;
        let log;
        match log {
            RpcTypes::UserLog { user, sub_id, log } => {
                addr = user;
                sub_id = sub_id;
                log = log;
            }
        }
        let raw_tran_op = self.actions.get(sub_id);
        let raw_tran;
        match raw_tran_op {
            Some(tran) => {
                raw_tran = tran;
            }
            None => {}
        }

        let tran_clone = raw_tran.clone();
        let Eth_Tran = tran_clone.build(log).unwrap();
        tokio::spawn(async move {});

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
