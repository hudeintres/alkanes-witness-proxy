use std::sync::Arc;

use alkanes_runtime::runtime::AlkaneResponder;
use alkanes_runtime::storage::StoragePointer;
use alkanes_runtime::{auth::AuthenticatedResponder, declare_alkane, message::MessageDispatch};
#[allow(unused_imports)]
use alkanes_runtime::{
    println,
    stdio::{stdout, Write},
};
use alkanes_support::cellpack::Cellpack;
use alkanes_support::id::AlkaneId;
use alkanes_support::parcel::AlkaneTransferParcel;
use alkanes_support::{context::Context, parcel::AlkaneTransfer, response::CallResponse};
use anyhow::{anyhow, Result};
use metashrew_support::compat::{to_arraybuffer_layout, to_passback_ptr};

#[derive(Default)]
pub struct DieselProxy(());

#[derive(MessageDispatch)]
enum DieselProxyMessage {
    #[opcode(69690420)]
    Initialize {},
}

impl DieselProxy {
    fn initialize(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        Ok(response)
    }
}

impl AlkaneResponder for DieselProxy {
    fn fallback(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let inputs: Vec<u128> = context.inputs.clone();
        let cellpack = Cellpack {
            target: AlkaneId::new(inputs[0], inputs[1]),
            inputs: inputs[2..].to_vec(),
        };
        let diesel_mint = self.call(
            &Cellpack {
                target: AlkaneId::new(2, 0),
                inputs: vec![77],
            },
            &AlkaneTransferParcel(vec![]),
            self.fuel(),
        )?;
        let mut arb_call = self.call(&cellpack, &context.incoming_alkanes, self.fuel())?;
        arb_call.alkanes.pay(diesel_mint.alkanes.0[0]);
        Ok(arb_call)
    }
}
// Use the new macro format
declare_alkane! {
    impl AlkaneResponder for DieselProxy {
        type Message = DieselProxyMessage;
    }
}
