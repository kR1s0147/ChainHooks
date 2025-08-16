use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::Address,
    providers::{
        DynProvider, Identity, Provider, ProviderBuilder, RootProvider, WalletProvider, WsConnect,
        fillers::{
            BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
            WalletFiller,
        },
    },
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::{EIP1186StorageProof, Filter, Log},
};

use futures::StreamExt;
use std::{cell::RefCell, default, error::Error, sync::Arc};

use tokio::{
    sync::{
        Mutex,
        mpsc::{self, Receiver},
        oneshot,
    },
    time,
};

use tokio_stream::StreamMap;
use tracing::subscriber;
pub mod rpc_types;
use dashmap::DashMap;
use rpc_types::*;
pub mod relayer;
use crate::{
    chainhooks::UserRegistrationResponse,
    rpchandler::{relayer::UserUpdates, transactionTypes::RawTransaction},
};
use std::collections::BTreeMap;
pub mod transactionTypes;

type providerType = FillProvider<
    JoinFill<
        JoinFill<
            JoinFill<
                Identity,
                JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
            >,
            ChainIdFiller,
        >,
        WalletFiller<EthereumWallet>,
    >,
    RootProvider,
>;

#[derive(Clone)]
pub struct chainRpc {
    chainid: usize,
    subscriptions: DashMap<Address, Vec<(String, SubscriptionType)>>,
    active_subscriptions: DashMap<String, SubscriptionType>,
    event_sender: mpsc::Sender<RpcTypes>,
    provider: Arc<Mutex<providerType>>,
    number_of_subsciptions: Arc<Mutex<usize>>,
}

impl chainRpc {
    async fn new(
        chainid: usize,
        URL: String,
        subscription: SubscriptionType,
        mut command_receiver: mpsc::Receiver<(SubscriptionType, oneshot::Sender<RpcTypes>)>,
        log_sender: mpsc::Sender<RpcTypes>,
    ) -> Result<(), Box<dyn Error>> {
        // Connecting the RPC node with web sockets

        let ws = WsConnect::new(URL);
        let wallet = EthereumWallet::default();

        let provider = ProviderBuilder::new()
            .with_chain_id(chainid.try_into().unwrap())
            .wallet(wallet)
            .connect_ws(ws)
            .await?;

        let (user, filter) = Self::getFilter(&subscription).unwrap();
        let sub = match provider.subscribe_logs(&filter).await {
            Ok(sub) => sub,
            Err(e) => return Err(Box::new(RpcTypeError::SubscriptionError)),
        };

        let key = sub.local_id().clone().to_string();

        let mut chainrpc = chainRpc {
            chainid: chainid,
            subscriptions: Default::default(),
            active_subscriptions: Default::default(),
            event_sender: log_sender,
            provider: Arc::new(Mutex::new(provider)),
            number_of_subsciptions: Arc::new(Mutex::new(1)),
        };

        chainrpc
            .active_subscriptions
            .insert(key.clone(), subscription.clone());

        chainrpc
            .subscriptions
            .insert(user, vec![(key.clone(), subscription.clone())]);

        let chain_rpc = Arc::new(chainrpc);
        let rpc_clone = chain_rpc.clone();
        let mut stream_map = StreamMap::new();

        stream_map.insert(key, sub.into_stream());

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    Some((cmd , res_receiver)) =  command_receiver.recv() => {
                        let mut rpc_cloned = rpc_clone.clone();
                        if let Err(e) = rpc_cloned.handlecmd(cmd ,res_receiver, &mut stream_map).await{
                            let res = RpcTypes::Response{
                                success : false,
                                message : "Error while handling the command".to_string(),
                            };
                            eprint!("cmd error :{e}");
                        }
                    }
                    Some((subid,event)) = stream_map.next() => {
                        let mut rpc_locked = rpc_clone.clone();
                        if let Err(e) = rpc_locked.handleevent(event,subid).await{
                            eprint!("cmd error :{e}");
                        }
                    }
                }
            }
        });
        Ok(())
    }

    fn getFilter(subscription: &SubscriptionType) -> Option<(Address, Filter)> {
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
                Some((user.clone(), filter))
            }
            _ => None,
        }
    }

    async fn handlecmd(
        &self,
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
                let (user, filter) = Self::getFilter(&cmd).unwrap();

                let sub = match self.provider.lock().await.subscribe_logs(&filter).await {
                    Ok(sub) => sub,
                    Err(e) => {
                        let res = RpcTypes::Response {
                            success: false,
                            message: "error while subscription".to_string(),
                        };
                        res_receiver.send(res);
                        return Err(Box::new(RpcTypeError::SubscriptionError));
                    }
                };

                *self.number_of_subsciptions.lock().await += 1;

                let subid = sub.local_id().to_string();

                self.active_subscriptions.insert(subid.clone(), cmd.clone());

                if let Some(mut subs) = self.subscriptions.get_mut(&user) {
                    subs.push((subid.clone(), cmd.clone()));
                } else {
                    self.subscriptions
                        .insert(user.clone(), vec![(subid.clone(), cmd.clone())]);
                }

                let res = RpcTypes::Response {
                    success: true,
                    message: subid.clone(),
                };
                res_receiver.send(res);
                stream_map.insert(subid, sub.into_stream());
            }

            SubscriptionType::Transaction {
                user,
                signer,
                tx,
                db,
            } => {
                let mut provider = self.provider.lock().await;
                let wallet = provider.wallet_mut();
                if let None = wallet.signer_by_address(signer.address()) {
                    wallet.register_signer(signer);
                }
                let provider_dup = provider.clone();
                // let res = provider.send_transaction(tx);
                tokio::spawn(async move {
                    if let Ok(tx_reciept) = provider_dup.send_transaction(tx).await {
                        if let Ok(receipt) = tx_reciept.get_receipt().await {
                            let tx_hash = receipt.transaction_hash;
                            let str = match serde_json::to_string(&receipt) {
                                Ok(t) => t,
                                Err(e) => {
                                    return Err(e);
                                }
                            };
                            if let Some(mut t) = db.get_mut(&user) {
                                let update = UserUpdates {
                                    Message: str.clone(),
                                    tx: tx_hash.to_string(),
                                };
                                let update = UserUpdates {
                                    Message: str,
                                    tx: tx_hash.to_string(),
                                };
                                t.insert(time::Instant::now(), update);
                            }
                        }
                    }
                    Ok(())
                });
            }

            SubscriptionType::Revoke_Sub { user, subs } => {
                self.active_subscriptions.remove(&subs);
                *self.number_of_subsciptions.lock().await -= 1;
                if let Some(mut user_subs) = self.subscriptions.get_mut(&user) {
                    user_subs.retain(|(s, _)| *s != subs);
                }
                stream_map.remove(&subs);
                res_receiver.send(RpcTypes::Response {
                    success: true,
                    message: "removed the subscription".to_string(),
                });
            }
        }

        Ok(())
    }

    async fn handleevent(&self, event: Log, subid: String) -> Result<(), Box<dyn Error>> {
        let subscription = self.active_subscriptions.get(&subid);
        match subscription {
            Some(sub) => match sub.clone() {
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
                    self.event_sender.send(rpcevent).await;
                }
                _ => return Ok(()),
            },
            None => return Err(Box::new(RpcTypeError::NoSubscriptionFound)),
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct RPChandler {
    pub available_chains: Vec<usize>,
    pub chain_state: DashMap<usize, ChainState>,
}

impl RPChandler {
    pub fn new(chains: Vec<usize>) -> Self {
        RPChandler {
            available_chains: chains,
            chain_state: Default::default(),
        }
    }

    pub fn new_chainstate(&mut self, chainid: usize, url: String) -> ChainState {
        let chain = ChainState {
            active: false,
            chain_url: url,
            channel: None,
        };
        self.chain_state.insert(chainid, chain.clone());
        chain
    }

    pub async fn build(
        &mut self,
        chainid: Vec<usize>,
        subscription: Vec<SubscriptionType>,
        log_sender: mpsc::Sender<RpcTypes>,
    ) -> Result<(), Box<dyn Error>> {
        let ziper = chainid.iter().zip(subscription.iter());
        for (chainid, sub) in ziper {
            self.new_conn(chainid.clone(), sub.clone(), log_sender.clone())
                .await?;
        }
        Ok(())
    }

    async fn new_conn(
        &mut self,
        chainid: usize,
        subscription: SubscriptionType,
        log_sender: mpsc::Sender<RpcTypes>,
    ) -> Result<(), Box<dyn Error>> {
        let chain_state = self.chain_state.get_mut(&chainid);
        match chain_state {
            Some(mut chainState) => {
                chainState.active = true;
                let (command_sender, command_receiver) =
                    mpsc::channel::<(SubscriptionType, oneshot::Sender<RpcTypes>)>(100);
                chainRpc::new(
                    chainid,
                    chainState.chain_url.clone(),
                    subscription,
                    command_receiver,
                    log_sender,
                )
                .await?;

                chainState.channel = Some(command_sender);

                return Ok(());
            }
            None => Err(Box::new(RpcTypeError::ChainNotSupported)),
        }
    }
}
