use alloy::json_abi::JsonAbi;
use alloy::primitives::{Address, address};
use alloy::rpc::types::TransactionRequest;
use alloy::rpc_types::Log;
use serde::de::Error;
use serde_json::Value;
use thiserror::Error;

pub struct TransactionBuilder {
    contract_address: Address,
    abi: JsonAbi,
    function_name: String,
    params: Vec<Value>,
}

impl TransactionBuilder {
    fn build(self, log: Log) -> Result<TransactionRequest, Box<dyn Error>> {}
}

#[derive(Error, Debug)]
pub enum RelayerError {
    #[error("Chain is Not Supported")]
    InvalidABI,
    #[error("Chain has no URL")]
    ChainHasNoRpcURL,
    #[error("User subscription is not found")]
    NoSubscriptionFound,
}

pub struct RawTransaction {
    contract_address: String,
    abi: String,
    function_name: String,
    params: Vec<(usize, String)>,
}

impl RawTransaction {
    fn build_transaction(self) -> Result<TransactionBuilder, Box<dyn Error>> {
        let contract_addr = Address::from_str(self.contract_address.as_str());
        let ABI: JsonAbi;
        match serde_json::from_str(&self.abi) {
            Ok(abi) => ABI = abi,
            Err(e) => return Err(Box::new(RelayerError::InvalidABI)),
        }
        let params = Vec::new();
        for (index, param) in self.params {
            if param.contains("topic") {
                let n = format!("topic{index}");
                params.push(n);
            } else {
                params.push(param);
            }
        }

        let tran = TransactionBuilder {
            contract_address: contract_addr,
            abi: ABI,
            function_name: self.function_name,
            params: params,
        };

        return Ok(tran);
    }
}
