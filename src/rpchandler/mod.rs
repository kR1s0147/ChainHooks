use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::Address,
    providers::{Provider, ProviderBuilder, WsConnect},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::{Filter, Log},
};

use futures::StreamExt;
use std::{error::Error, sync::Arc};

use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver},
    oneshot,
};

use tokio_stream::StreamMap;
use tracing::subscriber;
pub mod rpc_types;
use dashmap::DashMap;
use rpc_types::*;
pub mod relayer;

use crate::rpchandler::transactionTypes::RawTransaction;
pub mod transactionTypes;

pub struct chainRpc {
    chainid: usize,
    subscriptions: DashMap<Address, Vec<(String, SubscriptionType)>>,
    active_subscriptions: DashMap<String, SubscriptionType>,
    event_sender: mpsc::Sender<RpcTypes>,
    provider: Box<dyn Provider<Ethereum>>,
    number_of_subsciptions: usize,
    wallet: Arc<Mutex<EthereumWallet>>,
}

impl chainRpc {
    async fn new(
        chainid: usize,
        URL: String,
        subscription: SubscriptionType,
        command_receiver: mpsc::Receiver<SubscriptionType>,
        log_sender: mpsc::Sender<RpcTypes>,
    ) -> Result<(), Box<dyn Error>> {
        // Connecting the RPC node with web sockets

        let ws = WsConnect::new(URL);
        let wallet = Arc::new(EthereumWallet::default());
        let provider = ProviderBuilder::default()
            .with_recommended_fillers()
            .with_chain_id(chainid.try_into().unwrap())
            .connect_ws(ws)
            .wallet(wallet.lock().clone())
            .await?;

        let (user, filter) = Self::getFilter(&subscription);
        let sub = match provider.subscribe_logs(&filter).await {
            Ok(sub) => sub,
            Err(e) => return Err(Box::new(RpcTypeError::SubscriptionError)),
        };

        let key = sub.local_id().clone().to_string();

        let mut chainrpc = chainRpc {
            chainid: chainid,
            subscriptions: Default::default(),
            active_subscriptions: Default::default(),
            event_sender: rlog_sender,
            provider: Box::new(provider),
            number_of_subsciptions: 1,
            wallet: wallet,
        };

        chainrpc
            .active_subscriptions
            .insert(key.clone(), subscription.clone());

        chainrpc
            .subscriptions
            .insert(user, vec![(key.clone(), subscription.clone())]);

        let chain_rpc = Arc::new(Mutex::new(chainrpc));
        let rpc_clone = chain_rpc.clone();
        let mut stream_map = StreamMap::new();

        stream_map.insert(key, sub.into_stream());

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    Some((cmd , res_receiver)) =  command_receiver.recv() => {
                        let mut rpc_locked = rpc_clone.lock().await;
                        if let Err(e) = (&mut *rpc_locked).handlecmd(cmd ,res_receiver, &mut stream_map).await{
                            let res = RpcTypes::Response{
                                success : false,
                                message : "Error while handling the command",
                            };
                            res_receiver.send(res).await;
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
            _ => {}
        }
    }

    async fn handlecmd(
        &mut self,
        cmd: SubscriptionType,
        res_receiver: oneshot::Sender<RpcTypes>,
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
                let sub = match provider.subscribe_logs(&filter).await {
                    Ok(sub) => sub,
                    Err(e) => {
                        let res = RpcTypes::Response {
                            success: false,
                            message: "error while subscription",
                        };
                        res_receiver.send(res).await;
                        return Err(Box::new(RpcTypeError::SubscriptionError));
                    }
                };
                self.number_of_subsciptions += 1;

                let subid = sub.local_id().to_string();

                self.active_subscriptions.insert(subid.clone(), cmd.clone());

                if let Some(subs) = self.subscriptions.get_mut(user.clone()) {
                    subs.push((subid.clone(), cmd.clone()));
                } else {
                    self.subscriptions
                        .insert(user.clone(), vec![(subid.clone(), cmd.clone())])
                }

                stream_map.insert(subid, sub.into_stream());
            }

            SubscriptionType::Revoke_User { user } => {
                if let Some(subs) = self.subscriptions.get(user.clone()) {
                    for (sub_id, sub) in subs {
                        stream_map.remove(sub_id);
                        self.active_subscriptions.remove(sub_id);
                    }
                    self.subscriptions.remove(user.clone())
                }
            }
            SubscriptionType::Transaction { signer, tx } => {
                let wallet = self.wallet.lock().await;
                if let Some(signer) = wallet.signer_by_address(signer.address()) {
                    let res = self.provider.send_transaction(tx);
                    tokio::spawn(async move {
                        if let Ok(tx_reciept) = res.await {
                            if let Ok(receipt) = tx_reciept.get_receipt().await {
                                let str = match serde_json::to_string(receipt) {
                                    Ok(t) => t,
                                    Err(e) => {}
                                };
                                res_receiver.send(str);
                            }
                        }
                    })
                } else {
                    wallet.register_signer(signer);
                    let res = self.provider.send_transaction(tx);
                    tokio::spawn(async move {
                        if let Ok(tx_reciept) = res.await {
                            if let Ok(receipt) = tx_reciept.get_receipt().await {
                                let str = match serde_json::to_string(receipt) {
                                    Ok(t) => t,
                                    Err(e) => {}
                                };
                                res_receiver.send(str);
                            }
                        }
                    })
                }
            }

            _ => {}
        }

        let res = RpcTypes::Response {
            success: bool,
            message: "Command success",
        };
        res_receiver.send(res).await?;

        Ok(())
    }

    async fn handleevent(&mut self, event: Log, subid: String) -> Result<(), Box<dyn Error>> {
        let subscription = self.active_subscriptions.get(&subid);
        match subscription {
            Some(sub) => match sub {
                SubscriptionType::Subscription {
                    user,
                    chainid,
                    address,
                    event_signature,
                } => {
                    let rpcevent = RpcTypes::UserLog {
                        user: user.clone(),
                        sub_id: subid,
                        log: event,
                    };
                    self.log_sender.send(rpcevent).await?;
                }

                SubscriptionType::Revoke_User { user } => {}
            },
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

    fn new_chainstate(&mut self, chainid: usize, url: String) -> ChainState {
        let chain = ChainState {
            active: false,
            chain_url: url,
            channel: None,
        };
        self.chain_state.insert(chainid, chain);
    }

    async fn build(
        &mut self,
        chainid: Vec<usize>,
        subscription: Vec<SubscriptionType>,
        log_sender: mpsc::Sender<RpcTypes>,
    ) -> Result<(), Box<dyn Error>> {
        let ziper = chainid.iter().zip(subscription.iter());

        for (chainid, sub) in ziper {
            self.new_conn(chainid, sub, log_sender.clone()).await
        }
    }

    async fn new_conn(
        &mut self,
        chainid: usize,
        subscription: SubscriptionType,
        log_sender: mpsc::Sender<RpcTypes>,
    ) -> Result<(), Box<dyn Error>> {
        let chain_state = self.chain_state.get_mut(&chainid);
        match chain_state {
            Some(chainState) => {
                chainState.active = true;
                let (command_receiver, command_sender) = mpsc::channel::<SubscriptionType>(100);
                chainRpc::new(
                    chainid,
                    chainState.chain_url.clone(),
                    subscription,
                    command_receiver,
                    log_sender,
                )
                .await?;

                chainState.channel = Some(command_sender);

                return Ok(rpc_receiver);
            }
            None => Err(Box::new(RpcTypeError::ChainNotSupported)),
        }
    }
}
