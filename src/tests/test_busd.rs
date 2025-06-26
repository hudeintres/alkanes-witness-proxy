use crate::tests::std::bUSD_build;
use crate::BUSD_DEPLOYMENT_ID;
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
use protorune_support::balance_sheet::{BalanceSheet, BalanceSheetOperations};
use protorune_support::protostone::{Protostone, ProtostoneEdict};
use std::fmt::Write;
use std::str::FromStr;
use wasm_bindgen_test::wasm_bindgen_test;

// Helper function to create a block with a free-mint deployment
fn init_block_with_bUSD_deployment() -> Result<(bitcoin::Block, AlkaneId)> {
    let block_height = 840_000;

    // Initialize the free-mint contract
    let token_units = 1000u128;
    let value_per_mint = 10u128;
    let cap = 100u128;
    let name_part1 = 0x54534554u128; // "TEST" in little-endian
    let name_part2 = 0x32u128; // "2" in little-endian
    let symbol = 0x545354u128; // "TST" in little-endian

    let test_block = create_init_tx(
        token_units,
        value_per_mint,
        cap,
        name_part1,
        name_part2,
        symbol,
    );

    Ok((test_block, AlkaneId::new(4, BUSD_DEPLOYMENT_ID)))
}

// Helper function to create a transaction that initializes the free-mint contract
fn create_init_tx(
    token_units: u128,
    value_per_mint: u128,
    cap: u128,
    name_part1: u128,
    name_part2: u128,
    symbol: u128,
) -> bitcoin::Block {
    alkane_helpers::init_with_multiple_cellpacks_with_tx(
        vec![
            alkanes_std_auth_token_build::get_bytes(),
            bUSD_build::get_bytes(),
        ],
        vec![
            Cellpack {
                target: AlkaneId {
                    block: 3,
                    tx: AUTH_TOKEN_FACTORY_ID,
                },
                inputs: vec![100],
            },
            Cellpack {
                target: AlkaneId::new(3, BUSD_DEPLOYMENT_ID),
                // Initialize opcode (0) with parameters
                inputs: vec![0, 1, token_units],
            },
        ],
    )
}

pub enum CellpackOrEdict {
    Cellpack(Cellpack),
    Edict(Vec<ProtostoneEdict>),
}

pub fn create_multiple_cellpack_with_witness_and_in_with_edicts_and_leftovers(
    witness: Witness,
    cellpacks_or_edicts: Vec<CellpackOrEdict>,
    previous_output: OutPoint,
    etch: bool,
    with_leftovers_to_separate: bool,
) -> Transaction {
    let protocol_id = 1;
    let input_script = ScriptBuf::new();
    let txin = TxIn {
        previous_output,
        script_sig: input_script,
        sequence: Sequence::MAX,
        witness,
    };
    let protostones = [
        match etch {
            true => vec![Protostone {
                burn: Some(protocol_id),
                edicts: vec![],
                pointer: Some(5),
                refund: None,
                from: None,
                protocol_tag: 13, // this value must be 13 if protoburn
                message: vec![],
            }],
            false => vec![],
        },
        cellpacks_or_edicts
            .into_iter()
            .enumerate()
            .map(|(i, cellpack_or_edict)| match cellpack_or_edict {
                CellpackOrEdict::Cellpack(cellpack) => Protostone {
                    message: cellpack.encipher(),
                    pointer: Some(0),
                    refund: Some(0),
                    edicts: vec![],
                    from: None,
                    burn: None,
                    protocol_tag: protocol_id as u128,
                },
                CellpackOrEdict::Edict(edicts) => Protostone {
                    message: vec![],
                    pointer: if with_leftovers_to_separate {
                        Some(2)
                    } else {
                        Some(0)
                    },
                    refund: if with_leftovers_to_separate {
                        Some(2)
                    } else {
                        Some(0)
                    },
                    //lazy way of mapping edicts onto next protomessage
                    edicts: edicts
                        .into_iter()
                        .map(|edict| {
                            let mut edict = edict;
                            edict.output = if etch { 5 + i as u128 } else { 4 + i as u128 };
                            if with_leftovers_to_separate {
                                edict.output += 1;
                            }
                            edict
                        })
                        .collect(),
                    from: None,
                    burn: None,
                    protocol_tag: protocol_id as u128,
                },
            })
            .collect(),
    ]
    .concat();
    let etching = if etch {
        Some(Etching {
            divisibility: Some(2),
            premine: Some(1000),
            rune: Some(Rune::from_str("TESTTESTTESTTEST").unwrap()),
            spacers: Some(0),
            symbol: Some(char::from_str("A").unwrap()),
            turbo: true,
            terms: None,
        })
    } else {
        None
    };
    let runestone: ScriptBuf = (Runestone {
        etching,
        pointer: match etch {
            true => Some(1),
            false => Some(0),
        }, // points to the OP_RETURN, so therefore targets the protoburn
        edicts: Vec::new(),
        mint: None,
        protocol: protostones.encipher().ok(),
    })
    .encipher();

    //     // op return is at output 1
    let op_return = TxOut {
        value: Amount::from_sat(0),
        script_pubkey: runestone,
    };
    let address: Address<NetworkChecked> = get_address(&ADDRESS1().as_str());

    let script_pubkey = address.script_pubkey();
    let txout = TxOut {
        value: Amount::from_sat(100_000_000),
        script_pubkey: script_pubkey.clone(),
    };
    let outputs = if with_leftovers_to_separate {
        vec![
            txout,
            op_return,
            TxOut {
                value: Amount::from_sat(546),
                script_pubkey,
            },
        ]
    } else {
        vec![txout, op_return]
    };
    Transaction {
        version: Version::ONE,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![txin],
        output: outputs,
    }
}

pub fn create_multiple_cellpack_with_witness_and_in_with_edicts(
    witness: Witness,
    cellpacks_or_edicts: Vec<CellpackOrEdict>,
    previous_output: OutPoint,
    etch: bool,
) -> Transaction {
    create_multiple_cellpack_with_witness_and_in_with_edicts_and_leftovers(
        witness,
        cellpacks_or_edicts,
        previous_output,
        etch,
        false,
    )
}

// Helper function to create a transaction that mints tokens
fn create_redeem_tx(
    test_block: &mut bitcoin::Block,
    free_mint_deployment: AlkaneId,
    previous_outpoint: OutPoint,
) -> OutPoint {
    test_block
        .txdata
        .push(create_multiple_cellpack_with_witness_and_in_with_edicts(
            Witness::new(),
            vec![
                CellpackOrEdict::Edict(vec![ProtostoneEdict {
                    id: free_mint_deployment.into(),
                    amount: 100,
                    output: 0,
                }]),
                CellpackOrEdict::Cellpack(Cellpack {
                    target: free_mint_deployment,
                    inputs: vec![88, 1, 2, 3, 4],
                }),
                CellpackOrEdict::Cellpack(Cellpack {
                    target: free_mint_deployment,
                    inputs: vec![102, 0],
                }),
                CellpackOrEdict::Cellpack(Cellpack {
                    target: free_mint_deployment,
                    inputs: vec![103, 0],
                }),
            ],
            previous_outpoint,
            false,
        ));

    // Return the outpoint of the transaction we just added
    OutPoint {
        txid: test_block.txdata.last().unwrap().compute_txid(),
        vout: 0,
    }
}

fn get_sheet_for_outpoint(
    test_block: &bitcoin::Block,
    tx_num: usize,
    vout: u32,
) -> Result<BalanceSheet<IndexPointer>> {
    let outpoint = OutPoint {
        txid: test_block.txdata[tx_num].compute_txid(),
        vout,
    };
    let ptr = RuneTable::for_protocol(AlkaneMessageContext::protocol_tag())
        .OUTPOINT_TO_RUNES
        .select(&consensus_encode(&outpoint)?);
    let sheet = load_sheet(&ptr);
    println!(
        "balances at outpoint tx {} vout {}: {:?}",
        tx_num, vout, sheet
    );
    Ok(sheet)
}

pub fn get_last_outpoint_sheet(test_block: &bitcoin::Block) -> Result<BalanceSheet<IndexPointer>> {
    let len = test_block.txdata.len();
    get_sheet_for_outpoint(test_block, len - 1, 0)
}

// Helper function to get the balance of a token
fn get_token_balance(block: &bitcoin::Block, token_id: AlkaneId) -> Result<u128> {
    let sheet = get_last_outpoint_sheet(block)?;
    Ok(sheet.get_cached(&token_id.into()))
}

#[wasm_bindgen_test]
fn test_busd_redeem() -> Result<()> {
    clear();

    let block_height = 840_000;
    let (test_block, busd_deployment) = init_block_with_bUSD_deployment()?;

    // Index the block
    index_block(&test_block, block_height)?;

    // Check the token balance
    let balance = get_token_balance(&test_block, busd_deployment)?;
    assert_eq!(
        balance, 1000u128,
        "Initial token balance should match token_units"
    );

    let mut redeem_block = create_block_with_coinbase_tx(block_height);

    create_redeem_tx(
        &mut redeem_block,
        busd_deployment,
        OutPoint {
            txid: test_block.txdata.last().unwrap().compute_txid(),
            vout: 0,
        },
    );
    index_block(&redeem_block, block_height)?;

    // Get the trace data from the transaction
    let outpoint = OutPoint {
        txid: redeem_block.txdata[redeem_block.txdata.len() - 1].compute_txid(),
        vout: 5,
    };

    let trace_data: Trace = view::trace(&outpoint)?.try_into()?;

    let last_trace_event = trace_data.0.lock().expect("Mutex poisoned").last().cloned();

    // Access the data field from the trace response
    if let Some(return_context) = last_trace_event {
        // Use pattern matching to extract the data field from the TraceEvent enum
        match return_context {
            TraceEvent::ReturnContext(trace_response) => {
                // Now we have the TraceResponse, access the data field
                let data = &trace_response.inner.data;
                assert_eq!(data[0], 1);
                assert_eq!(data[16], 2);
                assert_eq!(data[32], 3);
                assert_eq!(data[48], 4);
            }
            _ => panic!("Expected ReturnContext variant, but got a different variant"),
        }
    } else {
        panic!("Failed to get last_trace_event from trace data");
    }

    // Get the trace data from the transaction
    let outpoint = OutPoint {
        txid: redeem_block.txdata[redeem_block.txdata.len() - 1].compute_txid(),
        vout: 6,
    };

    let trace_data: Trace = view::trace(&outpoint)?.try_into()?;

    let last_trace_event = trace_data.0.lock().expect("Mutex poisoned").last().cloned();

    // Access the data field from the trace response
    if let Some(return_context) = last_trace_event {
        // Use pattern matching to extract the data field from the TraceEvent enum
        match return_context {
            TraceEvent::ReturnContext(trace_response) => {
                // Now we have the TraceResponse, access the data field
                let data = &trace_response.inner.data;
                assert_eq!(data[0], 1);
            }
            _ => panic!("Expected ReturnContext variant, but got a different variant"),
        }
    } else {
        panic!("Failed to get last_trace_event from trace data");
    }

    Ok(())
}
