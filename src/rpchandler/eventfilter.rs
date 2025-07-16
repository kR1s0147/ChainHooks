use alloy::providers::{Provider, ProviderBuilder};
use std::collections::Hashmap;
use std::error::Error;

struct RPChandler {
    providers: Hashmap<String, Provider>,
    supportedChains: Hashmap<String, String>,
    filters: Hashmap<String, String>,
}

impl RPChandler {}
