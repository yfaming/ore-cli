use ore::{state::Bus, utils::AccountDeserialize, BUS_ADDRESSES};
use solana_client::client_error::Result;

use crate::Miner;

impl Miner {
    pub async fn busses(&self) {
        for address in BUS_ADDRESSES.iter() {
            let data = self.rpc_client.get_account_data(address).await.unwrap();
            match Bus::try_from_bytes(&data) {
                Ok(bus) => {
                    println!("Bus {}: {:} ORE", bus.id, bus.rewards);
                }
                Err(_) => {}
            }
        }
    }

    pub async fn get_bus(&self, id: usize) -> Result<Bus> {
        let data = self.rpc_client.get_account_data(&BUS_ADDRESSES[id]).await?;
        Ok(*Bus::try_from_bytes(&data).unwrap())
    }
}
