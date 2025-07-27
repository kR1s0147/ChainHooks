use alloy::network::Ethereum;
use alloy::primitives::{Address, keccak256};
use alloy::providers::RootProvider;

use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use alloy::pubsub::{Subscription, SubscriptionStream};
use alloy::rpc;
use alloy::rpc::types::Filter;
use alloy::rpc::types::Log;
use futures::StreamExt;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{self, Receiver};
use tokio_stream::StreamMap;
use tracing::subscriber;
pub mod rpc_types;
use dashmap::DashMap;
use rpc_types::*;
mod relayer;

use crate::rpchandler::transactionTypes::RawTransaction;

pub mod transactionTypes;
pub struct chainRpc {
    chainid: usize,
    subscriptions: DashMap<Address, Vec<(String, SubscriptionType)>>,
    active_subscriptions: DashMap<String, SubscriptionType>,
    event_sender: mpsc::Sender<RpcTypes>,
    provider: Box<dyn Provider<Ethereum>>,
    number_of_subsciptions: usize,
}

impl chainRpc {
    async fn new(
        chainid: usize,
        URL: String,
        subscription: SubscriptionType,
    ) -> Result<(mpsc::Sender<SubscriptionType>, mpsc::Receiver<RpcTypes>), Box<dyn Error>> {
        // Connecting the RPC node with web sockets

        let ws = WsConnect::new(URL);

        let provider = ProviderBuilder::new()
            .with_chain_id(chainid.try_into().unwrap())
            .connect_ws(ws)
            .await?;

        let (command_sender, mut command_receiver) = mpsc::channel::<SubscriptionType>(100);
        let (rpcevent_sender, rpcevent_receiver) = mpsc::channel::<RpcTypes>(100);

        let (user, filter) = Self::getFilter(&subscription);
        let sub = provider.subscribe_logs(&filter).await?;
        let key = sub.local_id().clone().to_string();

        let mut chainrpc = chainRpc {
            chainid: chainid,
            subscriptions: Default::default(),
            active_subscriptions: Default::default(),
            event_sender: rpcevent_sender,
            provider: Box::new(provider),
            number_of_subsciptions: 1,
        };

        chainrpc
            .active_subscriptions
            .insert(key.clone(), subscription.clone());

        let chain_rpc = Arc::new(Mutex::new(chainrpc));
        let rpc_clone = chain_rpc.clone();
        let mut stream_map = StreamMap::new();

        stream_map.insert(key, sub.into_stream());

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    Some(cmd) =  command_receiver.recv() => {
                        let mut rpc_locked = rpc_clone.lock().await;
                        if let Err(e) = (&mut *rpc_locked).handlecmd(cmd , &mut stream_map).await{
                            eprint!("cmd error :{e}");
                        }
                    }
                    Some((subid,event)) = stream_map.next() => {
                        let mut rpc_locked = rpc_clone.lock().await;
                        if let Err(e) = (&mut *rpc_locked).handleevent(event,subid).await{
                            eprint!("cmd error :{e}");
                        }
                    }
                }
            }
        });

        Ok((command_sender, rpcevent_receiver))
    }

    fn getFilter(subscription: &SubscriptionType) -> (Address, Filter) {
        match subscription {
            SubscriptionType::Subscription {
                user,
                chainid,
                address,
                event_signature,
            } => {
                let filter = Filter::new()
                    .address(address.clone())
                    .events(event_signature);
                (user.clone(), filter)
            }
        }
    }

    async fn handlecmd(
        &mut self,
        cmd: SubscriptionType,
        stream_map: &mut StreamMap<String, SubscriptionStream<Log>>,
    ) -> Result<(), Box<dyn Error>> {
        match cmd.clone() {
            SubscriptionType::Subscription {
                user,
                chainid,
                address,
                event_signature,
            } => {
                let (user, filter) = Self::getFilter(&cmd);
                let sub = self.provider.subscribe_logs(&filter).await?;
                self.number_of_subsciptions += 1;

                let subid = sub.local_id().to_string();
                self.active_subscriptions.insert(subid.clone(), cmd.clone());

                let mut usersubs = self.subscriptions.get_mut(&user);

                match usersubs {
                    Some(subs) => {
                        subs.push((subid.clone(), cmd));
                    }
                    None => {
                        let d = vec![(subid.clone(), cmd)];
                        self.subscriptions.insert(user, d);
                    }
                }

                stream_map.insert(subid, sub.into_stream());
            }
        }
        Ok(())
    }

    async fn handleevent(&mut self, event: Log, subid: String) -> Result<(), Box<dyn Error>> {
        let subscription = self.active_subscriptions.get(&subid);
        match subscription {
            Some(sub) => {
                let event_string = serde_json::to_string(&event).unwrap();
                match sub {
                    SubscriptionType::Subscription {
                        user,
                        chainid,
                        address,
                        event_signature,
                    } => {
                        let rpcevent = RpcTypes::UserLog {
                            user: user.clone(),
                            log: event_string,
                        };
                        self.event_sender.send(rpcevent).await?;
                    }
                }
            }
            None => return Err(Box::new(RpcTypeError::NoSubscriptionFound)),
        }
        Ok(())
    }
}

struct RPChandler {
    available_chains: Vec<usize>,
    chain_state: DashMap<usize, ChainState>,
}

impl RPChandler {
    fn new() -> Self {
        RPChandler {
            available_chains: Vec::new(),
            chain_state: Default::default(),
        }
    }

    fn new_chainstate(chainid: usize, url: String) -> ChainState {
        ChainState {
            active: false,
            chain_url: url,
            channel: None,
        }
    }
    async fn build(
        &mut self,
        chainid: usize,
        subscription: SubscriptionType,
    ) -> Result<Receiver<RpcTypes>, Box<dyn Error>> {
        self.new_conn(chainid, subscription).await
    }

    async fn new_conn(
        &mut self,
        chainid: usize,
        subscription: SubscriptionType,
    ) -> Result<mpsc::Receiver<RpcTypes>, Box<dyn Error>> {
        let chain_state = self.chain_state.get_mut(&chainid);
        match chain_state {
            Some(chainState) => {
                chainState.active = true;
                let (command_sender, rpc_receiver) =
                    chainRpc::new(chainid, chainState.chain_url.clone(), subscription).await?;
                chainState.channel = Some(command_sender);

                return Ok(rpc_receiver);
            }
            None => Err(Box::new(RpcTypeError::ChainNotSupported)),
        }
    }
}
