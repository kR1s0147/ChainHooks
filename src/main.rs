#![allow(warnings)]
use crate::rpchandler::rpc_types::SubscriptionType;

use tonic::{Request, Response, Status, transport::Server};
mod relayer;
mod rpchandler;

pub mod chainhooks {
    tonic::include_proto!("chainhooks");
}

#[tokio::main]
async fn main() {}
