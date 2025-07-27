use alloy::{primitives::Address, rpc::types::Log};
use dashmap::DashMap;
use tokio::{sync::mpsc, time};

pub struct RelayerHandler {
    log_receiver: mpsc::Receiver<Log>,
    command_reciever: mpsc::Receiver<RelayerCommand>,
    relayers: DashMap<Address, EthereumWallet>,
    user_logs: DashMap<Address, Vec<UserLogs>>,
    actions: DashMap<String, RawTransaction>,
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
                        self.handle_command()?
                    }
                    log = log_receiver.recv() => {
                        self.handle_log()?
                    }
                }
            }
        })
    }

    async fn handle_command(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {}
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

pub struct UserLogs {
    Message: String,
    time: time::Instant,
}
