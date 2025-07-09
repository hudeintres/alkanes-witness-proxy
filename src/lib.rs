use std::sync::Arc;

use alkanes_runtime::runtime::AlkaneResponder;
use alkanes_runtime::storage::StoragePointer;
use alkanes_runtime::{auth::AuthenticatedResponder, declare_alkane, message::MessageDispatch};
#[allow(unused_imports)]
use alkanes_runtime::{
    println,
    stdio::{stdout, Write},
};
use alkanes_std_factory_support::MintableToken;
use alkanes_support::{context::Context, parcel::AlkaneTransfer, response::CallResponse};
use anyhow::{anyhow, Result};
use bitcoin::hashes::Hash;
use bitcoin::{Transaction, Txid};
use metashrew_support::compat::{to_arraybuffer_layout, to_passback_ptr};
use metashrew_support::index_pointer::KeyValuePointer;
use metashrew_support::utils::consensus_decode;

pub const BUSD_DEPLOYMENT_ID: u128 = 0xb05d;

#[cfg(test)]
pub mod tests;

pub struct RedeemInfo {
    pub amount: u128,
    pub token_id: u128,               // target usdc or usdt
    pub destination_chain_id: u128,   // target chain
    pub destination_address_h1: u128, // first 16 bytes of evm address
    pub destination_address_h2: u128, // last 4 bytes of evm address,
    pub txid: Txid,
}

impl RedeemInfo {
    pub fn try_to_vec(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&self.amount.to_le_bytes());
        bytes.extend_from_slice(&self.token_id.to_le_bytes());
        bytes.extend_from_slice(&self.destination_chain_id.to_le_bytes());
        bytes.extend_from_slice(&self.destination_address_h1.to_le_bytes());
        bytes.extend_from_slice(&self.destination_address_h2.to_le_bytes());
        bytes.extend_from_slice(&self.txid.as_byte_array().to_vec());

        bytes
    }
}

#[derive(Default)]
pub struct bUSD(());

impl MintableToken for bUSD {}

impl AuthenticatedResponder for bUSD {}

#[derive(MessageDispatch)]
enum bUSDMessage {
    #[opcode(0)]
    Initialize {
        auth_token_units: u128,
        token_units: u128,
    },

    #[opcode(1)]
    InitializeWithNameSymbol {
        auth_token_units: u128,
        token_units: u128,
        name: String,
        symbol: String,
    },

    #[opcode(77)]
    Mint { token_units: u128 },

    #[opcode(88)]
    Redeem {
        token_id: u128,               // target usdc or usdt
        destination_chain_id: u128,   // target chain
        destination_address_h1: u128, // first 16 bytes of evm address
        destination_address_h2: u128, // last 4 bytes of evm address,
    },

    #[opcode(99)]
    #[returns(String)]
    GetName,

    #[opcode(100)]
    #[returns(String)]
    GetSymbol,

    #[opcode(101)]
    #[returns(u128)]
    GetTotalSupply,

    // Get redeem info by index
    // Redeem data returns { tx id, redeem's params }
    #[opcode(102)]
    #[returns(Vec<u8>)]
    GetRedeemInfoByIndex { index: u128 },

    /// Get the total redeem count
    #[opcode(103)]
    #[returns(u128)]
    GetTotalRedeemCount {},

    #[opcode(1000)]
    #[returns(Vec<u8>)]
    GetData,
}

impl bUSD {
    fn redeem_ptr(&self, index: u128) -> StoragePointer {
        StoragePointer::from_keyword("/redeem/").select_value::<u128>(index)
    }
    fn set_redeem_info(&self, index: u128, redeem_info: RedeemInfo) {
        self.redeem_ptr(index)
            .set(Arc::new(redeem_info.try_to_vec()));
    }
    fn redeem_count_ptr(&self) -> StoragePointer {
        StoragePointer::from_keyword("/redeem_count")
    }
    fn get_redeem_count(&self) -> u128 {
        self.redeem_count_ptr().get_value::<u128>()
    }
    fn set_redeem_count(&self, v: u128) {
        self.redeem_count_ptr().set_value::<u128>(v);
    }
    fn increment_redeem_count(&self) {
        let current = self.get_redeem_count();
        self.set_redeem_count(current + 1);
    }
    fn initialize(&self, auth_token_units: u128, token_units: u128) -> Result<CallResponse> {
        self.initialize_with_name_symbol(
            auth_token_units,
            token_units,
            String::from("bUSD"),
            String::from("bUSD"),
        )
    }

    fn initialize_with_name_symbol(
        &self,
        auth_token_units: u128,
        token_units: u128,
        name: String,
        symbol: String,
    ) -> Result<CallResponse> {
        self.observe_initialization()?;
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        <Self as MintableToken>::set_name_and_symbol_str(self, name, symbol);
        self.set_data()?;
        response
            .alkanes
            .0
            .push(self.deploy_auth_token(auth_token_units)?);

        response
            .alkanes
            .pay(<Self as MintableToken>::mint(self, &context, token_units)?);

        self.set_redeem_count(0);

        Ok(response)
    }

    fn mint(&self, token_units: u128) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        self.only_owner()?;

        // Call the mint method from the MintableToken trait
        let transfer = <Self as MintableToken>::mint(self, &context, token_units)?;
        response.alkanes.pay(transfer);

        Ok(response)
    }

    fn redeem(
        &self,
        token_id: u128,               // target usdc or usdt
        destination_chain_id: u128,   // target chain
        destination_address_h1: u128, // first 16 bytes of evm address
        destination_address_h2: u128, // last 4 bytes of evm address,
    ) -> Result<CallResponse> {
        let context = self.context()?;
        if context.incoming_alkanes.0.len() != 1 {
            return Err(anyhow!("Input must be 1 alkane"));
        }
        if context.myself != context.incoming_alkanes.0[0].id {
            return Err(anyhow!("Input must be owned token"));
        }

        self.decrease_total_supply(context.incoming_alkanes.0[0].value)?;

        let curr_index = self.get_redeem_count();
        let tx = consensus_decode::<Transaction>(&mut std::io::Cursor::new(self.transaction()))?;
        let redeem_info = RedeemInfo {
            amount: context.incoming_alkanes.0[0].value,
            token_id: token_id,
            destination_chain_id: destination_chain_id,
            destination_address_h1: destination_address_h1,
            destination_address_h2: destination_address_h2,
            txid: tx.compute_txid(),
        };
        self.set_redeem_info(curr_index, redeem_info);

        self.increment_redeem_count();

        Ok(CallResponse::default())
    }

    fn get_name(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = self.name().into_bytes().to_vec();

        Ok(response)
    }

    fn get_symbol(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = self.symbol().into_bytes().to_vec();

        Ok(response)
    }

    fn get_redeem_info_by_index(&self, index: u128) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = self.redeem_ptr(index).get().to_vec();

        Ok(response)
    }

    fn get_total_redeem_count(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = self.get_redeem_count().to_le_bytes().to_vec();

        Ok(response)
    }

    fn get_total_supply(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = self.total_supply().to_le_bytes().to_vec();

        Ok(response)
    }

    fn get_data(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = self.data();

        Ok(response)
    }
}

impl AlkaneResponder for bUSD {}

// Use the new macro format
declare_alkane! {
    impl AlkaneResponder for bUSD {
        type Message = bUSDMessage;
    }
}
