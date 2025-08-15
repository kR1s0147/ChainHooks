use alloy::dyn_abi::{DynSolType, DynSolValue, JsonAbiExt};
use alloy::json_abi::{Function, JsonAbi, Param};
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, FixedBytes, address};
use alloy::rpc::types::Log;
use alloy::rpc::types::TransactionRequest;
use serde_json::Value;
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RelayerError {
    #[error("Chain is Not Supported")]
    InvalidABI,

    #[error("Chain has no URL")]
    ChainHasNoRpcURL,

    #[error("User subscription is not found")]
    NoSubscriptionFound,

    #[error("invalid transaction")]
    InvalidTransactionRequest,

    #[error("Function not found")]
    FunctionNotFound,

    #[error("Invalid Data Type")]
    InvalidDataType,

    #[error("Invalid topic mapping")]
    InvalidTopicMapping,

    #[error("Invalid args count")]
    InvalidArgsCount,

    #[error("Topic Range Out of Index")]
    TopicOutOfIndex,

    #[error("Invalid address")]
    InvalidAddress,

    #[error("Already Registered")]
    AlreadyRegistered,
}

pub enum UserUpdates {}

#[derive(Clone)]
pub struct RawTransaction {
    pub chain_id: usize,
    pub contract_address: String,
    abi: String,
    function_name: String,
    params: Vec<(usize, String)>,
}

impl RawTransaction {
    pub fn new(
        chain_id: usize,
        contract_address: String,
        abi: String,
        function_name: String,
        params: Vec<(usize, String)>,
    ) -> Self {
        RawTransaction {
            chain_id,
            contract_address,
            abi,
            function_name,
            params,
        }
    }
    pub fn build_transaction(
        self,
        log: Log,
    ) -> Result<TransactionRequest, Box<dyn std::error::Error>> {
        let contract_addr = Address::from_slice(self.contract_address.as_bytes());
        let ABI: JsonAbi;

        match serde_json::from_str(&self.abi) {
            Ok(abi) => ABI = abi,
            Err(e) => return Err(Box::new(RelayerError::InvalidABI)),
        }

        let function_val = ABI.function(&self.function_name).unwrap().first();
        let function: Function;

        match function_val {
            Some(func) => {
                function = func.clone();
            }
            None => return Err(Box::new(RelayerError::FunctionNotFound)),
        }

        let topics = log.topics();
        let mut resolved_params: Vec<String> = Vec::new();

        for (paramnum, param_str) in &self.params {
            if param_str.starts_with("topic") {
                let index_str = &param_str[5..];
                let index: usize = index_str.parse().unwrap();
                let topic_value = topics.get(index);
                let topic: &FixedBytes<32>;
                match topic_value {
                    Some(t) => topic = t,
                    None => return Err(Box::new(RelayerError::InvalidTopicMapping)),
                }

                resolved_params.push(topic.to_string());
            } else {
                resolved_params.push(param_str.clone());
            }
        }
        let sol_values = self
            .convert_strings_to_sol_values(&resolved_params, &function.inputs)
            .unwrap();
        let data = function.abi_encode_input(&sol_values).unwrap();

        let transaction = TransactionRequest::default()
            .to(contract_addr)
            .with_input(Bytes::from(data));

        Ok(transaction)
    }

    fn convert_strings_to_sol_values<'a>(
        &self,
        params: &[String],
        abi_inputs: &'a [Param],
    ) -> Result<Vec<DynSolValue>, Box<dyn std::error::Error>> {
        if params.len() != abi_inputs.len() {
            return Err(Box::new(RelayerError::InvalidArgsCount));
        }
        let param_sols = params.iter().zip(abi_inputs.iter());
        let mut solval: Vec<DynSolValue> = Vec::new();
        for (param, abi_in) in param_sols {
            let sol_type: DynSolType = match abi_in.ty.parse() {
                Ok(solv) => solv,
                Err(_) => return Err(Box::new(RelayerError::InvalidDataType)),
            };

            let solvalue = sol_type.coerce_str(&param);
            match solvalue {
                Ok(sol) => {
                    solval.push(sol);
                }
                Err(_) => return Err(Box::new(RelayerError::InvalidDataType)),
            }
        }

        Ok(solval)
    }
}
