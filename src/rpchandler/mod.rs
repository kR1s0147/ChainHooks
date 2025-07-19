#[allow(warnings)]
use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use alloy::transports::RpcError;
use std::collections::{HashMap, Hashmap};
use std::error::Error;
use tokio::sync::mpsc;

mod rpc_types;
use rpc_types::*;

struct chainRpc {
    chainid: usize,
    subscriptions: Hashmap<Address, Vec<SubscriptionType>>,
    event_sender: mpsc::Sender<RpcTypes>,
}

impl chainRpc {
    fn new(chainid: String) -> Self {
        chainRpc {
            chainid: chainid,
            subscriptions: HashMap::new(),
            event_sender: (),
        }
    }

    async fn run(&mut self, URL: String) -> Result<(), Box<dyn Error>> {
        let ws = WsConnect::new(URL);
        let provider = ProviderBuilder::new()
            .with_chain_id(chain_id)
            .connect_ws(ws)
            .await?;
        let (command_sender, command_receiver) = mpsc::channel::<SubscriptionType>(100);
        let (rpcevent_sender, rpcevent_receiver) = mpsc::channel::<RpcTypes>(100);

        tokio::task::spawn(async move || {
            loop {
                tokio::select! {
                    cmd =  command_receiver.recv().await? => {
                        handlecmd(self,cmd).await?;
                    }

                    event = rpcevent_receiver.recv().await? => {
                        handleevent(self,event).await?
                    }
                }
            }
        })
    }

    async fn handlecmd(&self, cmd: SubscriptionType) -> Result<(), Box<dyn Error>> {
        match cmd {
            SubscriptionType::Subscription {
                user,
                chainid,
                address,
                event_signature,
                topics,
            } => {}
        }
    }
}

struct RPChandler {
    available_chains: Vec<usize>,
    providers: Hashmap<usize, mpsc::Sender<SubscriptionType>>,
    chain_url: Hashmap<usize, String>,
    user_filters: Hashmap<Address, String>,
}

impl RPChandler {
    fn new() -> Self {
        RPChandler {
            available_chains: Vec::new(),
            providers: Hashmap::new(),
            chain_url: Hashmap::new(),
            user_filters: Hashmap::new(),
        }
    }

    async fn build(&mut self, chainid: usize) -> Result<bool, Box<dyn Error>> {
        if self.available_chains.len() < 0 {
            return Ok(false);
        }

        for len in self.available_chains {
            (command_sender, command_receiver) = self.new_conn(chainid).await?;
        }
    }

    async fn new_conn(
        &self,
        chainid: usize,
    ) -> Result<(mpsc::Sender<SubscriptionType>, mpsc::Receiver<RpcTypes>), Box<dyn Error>> {
        if !self.isChainSupported(chainid) {
            return Err(RpcError::ChainNotSupported);
        }

        let url = self.getURL(chainid);
        let URL = String::new();
        match url {
            Some(url) => URL = url,
            None => return Err(RpcError::ChainHasNoRpcURl),
        }

        let chainrpc = chainRpc::new(chainid);
        chainrpc.run(URL).await?;
    }

    fn isChainSupported(&self, chainid: usize) -> bool {
        let url = self.chain_url.get(chainid);
        match url {
            Some(_) => return true,
            None => return false,
        }
    }

    fn getURL(&self, chainid: usize) -> Option<String> {
        let url = self.chain_url.get(chainid);
        match url {
            Some(url) => return Some(url),
            None => return None,
        }
    }
}
