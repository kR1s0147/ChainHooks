use alloy::{
    network::EthereumWallet,
    primitives::Address,
    signers::local::{LocalSigner, PrivateKeySigner},
};
use std::{collections::HashMap, error::Error};

use super::*;
use transaactionTypes::*;

fn new_relayer(address: String) -> Result<bool, Box<dyn Error>> {
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

    Ok(true)
}
