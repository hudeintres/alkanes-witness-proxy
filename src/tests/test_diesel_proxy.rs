use crate::tests::std::diesel_proxy_build;
use alkanes::indexer::index_block;
use alkanes::message::AlkaneMessageContext;
use alkanes::precompiled::alkanes_std_auth_token_build;
use alkanes::tests::helpers::{self as alkane_helpers, clear};
use alkanes::view;
use alkanes_support::cellpack::Cellpack;
use alkanes_support::constants::AUTH_TOKEN_FACTORY_ID;
use alkanes_support::id::AlkaneId;
use alkanes_support::response::ExtendedCallResponse;
use alkanes_support::trace::{Trace, TraceEvent};
use anyhow::Result;
use bitcoin::hashes::Hash;

use bitcoin::address::NetworkChecked;
use bitcoin::blockdata::transaction::OutPoint;
use bitcoin::transaction::Version;
use bitcoin::{Address, Amount, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness};
use metashrew_core::{get_cache, index_pointer::IndexPointer, println, stdio::stdout};
use metashrew_support::index_pointer::KeyValuePointer;
use metashrew_support::utils::consensus_encode;
use ordinals::{Etching, Rune, Runestone};
use protorune::balance_sheet::load_sheet;
use protorune::message::MessageContext;
use protorune::protostone::Protostones;
use protorune::tables::RuneTable;
use protorune::test_helpers::{create_block_with_coinbase_tx, get_address, ADDRESS1};
use protorune_support::balance_sheet::{BalanceSheet, BalanceSheetOperations, ProtoruneRuneId};
use protorune_support::protostone::{Protostone, ProtostoneEdict};
use std::fmt::Write;
use std::str::FromStr;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn test_diesel_proxy() -> Result<()> {
    clear();
    let block_height = 880_000;

    // Create a cellpack to call the process_numbers method (opcode 11)
    let init_test_cellpack = Cellpack {
        target: AlkaneId { block: 1, tx: 0 },
        inputs: vec![50],
    };
    let init_cellpack = Cellpack {
        target: AlkaneId { block: 3, tx: 1 },
        inputs: vec![69690420],
    };

    let test_build = include_bytes!("./precompiled/alkanes_std_test.wasm").to_vec();

    // Initialize the contract and execute the cellpacks
    let mut test_block = alkane_helpers::init_with_multiple_cellpacks_with_tx(
        [test_build, diesel_proxy_build::get_bytes()].into(),
        [init_test_cellpack, init_cellpack].into(),
    );

    // Create a cellpack to call the process_numbers method (opcode 11)
    let arb_mint_cellpack = Cellpack {
        target: AlkaneId { block: 4, tx: 1 },
        inputs: vec![2, 1, 22, 100000],
    };

    test_block
        .txdata
        .push(alkane_helpers::create_multiple_cellpack_with_witness(
            Witness::new(),
            vec![arb_mint_cellpack],
            false,
        ));

    index_block(&test_block, block_height)?;

    let sheet = alkane_helpers::get_last_outpoint_sheet(&test_block)?;

    println!("Last sheet: {:?}", sheet);

    assert_eq!(
        sheet.get_cached(&ProtoruneRuneId { block: 2, tx: 0 }),
        312500000
    );
    assert_eq!(
        sheet.get_cached(&ProtoruneRuneId { block: 2, tx: 1 }),
        100000
    );

    Ok(())
}
