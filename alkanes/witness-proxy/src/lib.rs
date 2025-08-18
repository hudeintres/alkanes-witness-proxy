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
use alkanes_support::witness::find_witness_payload;
use alkanes_support::{context::Context, parcel::AlkaneTransfer, response::CallResponse};
use anyhow::{anyhow, Result};
use bitcoin::hashes::Hash;
use bitcoin::{Transaction, Txid};
use metashrew_support::compat::{to_arraybuffer_layout, to_passback_ptr};
use metashrew_support::index_pointer::KeyValuePointer;
use metashrew_support::utils::consensus_decode;
use protorune_support::utils::decode_varint_list;
use std::io::Cursor;

#[derive(Default)]
pub struct WitnessProxy(());

impl MintableToken for WitnessProxy {}

impl AuthenticatedResponder for WitnessProxy {}

#[derive(MessageDispatch)]
enum WitnessProxyMessage {
    #[opcode(69690)]
    Initialize {},
}

impl WitnessProxy {
    fn initialize(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        Ok(response)
    }
}

pub fn extract_witness_payload(tx: &Transaction) -> Option<Vec<u8>> {
    // Try every input; Ordinals conventionally uses index 0, but
    // looping covers edge‑cases.
    for idx in 0..tx.input.len() {
        if let Some(data) = find_witness_payload(&tx, idx) {
            if !data.is_empty() {
                return Some(data);
            }
        }
    }
    None
}

impl AlkaneResponder for WitnessProxy {
    fn fallback(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let tx = self.transaction_object()?;
        let witness_payload = match extract_witness_payload(&tx) {
            Some(bytes) => bytes,
            None => return Err(anyhow!("Failed to decode tx witness")),
        };
        let cellpack = decode_varint_list(&mut Cursor::new(witness_payload.clone()))?.try_into()?;
        self.call(&cellpack, &context.incoming_alkanes, self.fuel())
    }
}
// Use the new macro format
declare_alkane! {
    impl AlkaneResponder for WitnessProxy {
        type Message = WitnessProxyMessage;
    }
}
