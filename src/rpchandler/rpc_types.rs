use alloy::primitives::{Address, ChainId};
use alloy::rpc::types::Filter;

struct Subscription {
    chainID: usize,
    address: Address,
    event_signature: String,
    topics: Option<Vec<String>>,
}

impl Subscription {
    fn new(
        chainID: String,
        address: String,
        event_signature: String,
        topics: Option<Vec<String>>,
    ) -> Self {
        return Subscription {
            chainID,
            address: Address::from_str(addres.as_str()),
            event_signature,
            topics,
        };
    }

    fn filter(self) -> Filter {
        let filter = Filter::new();
        filter
            .address(self.address)
            .event(&self.event_signature.as_str())
            .topics
    }
}
