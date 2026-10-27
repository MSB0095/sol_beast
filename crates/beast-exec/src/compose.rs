//! Multi-leg arbitrage composition.
//!
//! The straightforward way to run an N-leg arbitrage is N Jupiter swap
//! transactions inside a Jito bundle. That collides with Jito's five-transaction
//! bundle cap, so with a guard and a tip the cycle is limited to three legs.
//!
//! This module removes the cap differently: instead of N *transactions*, it
//! assembles N legs of raw instructions into **one** transaction. The bundle
//! becomes `[cycle, guard, tip]` - three transactions regardless of leg count -
//! and the real limit becomes Solana's 1232-byte transaction size.
//!
//! Cost consequences that matter for the green-or-revert guarantee:
//! * a composed cycle pays **one** base network fee instead of N;
//! * one transaction pays one priority fee instead of N;
//! * intermediate token accounts still cost rent, exactly as before;
//! * fewer transactions means less lamport spent before the guard runs, so the
//!   guard threshold keeps the same shape but with a lower `fees` term.
//!
//! Instruction hygiene rules enforced here:
//! * the compute-budget preamble of later legs is dropped (one budget per tx);
//! * intermediate `cleanupInstruction`s are dropped, because leg N+1 depends on
//!   the token account leg N just created;
//! * the final leg's cleanup runs so wrapped SOL is unwrapped before the guard;
//! * the wallet's own accounts are never touched except through the swaps.

use base64::Engine as _;
use beast_dex::jupiter::{RawInstruction, SwapInstructions};
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::message::compiled_instruction::CompiledInstruction;
use solana_sdk::message::v0::{Message as V0Message, MessageAddressTableLookup};
use solana_sdk::message::{MessageHeader, VersionedMessage};
use solana_sdk::transaction::{Transaction, VersionedTransaction};
use thiserror::Error;

/// Solana's hard cap on a serialized legacy transaction.
pub const MAX_TRANSACTION_BYTES: usize = 1232;
/// Legacy (non-ALT) encoding: 1 signature of 64 bytes + header + 1s per account key.
/// A conservative estimate of the per-account cost inside an instruction.

#[derive(Debug, Error)]
pub enum ComposeError {
    #[error("no legs were supplied")]
    Empty,
    #[error("leg {index} has no swap instruction")]
    MissingSwap { index: usize },
    #[error("leg {index} instruction data is not valid base64: {source}")]
    InstructionData {
        index: usize,
        #[source]
        source: base64::DecodeError,
    },
    #[error("leg {index} references a pubkey the engine cannot parse")]
    BadProgramId { index: usize },
    #[error("leg {index} references an account pubkey the engine cannot parse")]
    BadAccountId { index: usize },
    #[error("leg {index} requires an additional signer; only the wallet keypair is held")]
    UnsupportedSigner { index: usize },
    #[error(
        "composed cycle needs {needed} bytes, exceeding the {MAX_TRANSACTION_BYTES}-byte transaction limit"
    )]
    TooLarge { needed: usize },
    #[error("transaction serialization failed: {0}")]
    Serialize(String),
    #[error("more than 64 static accounts; the composed cycle needs fewer legs")]
    TooManyStaticKeys,
    #[error("an account is neither static nor present in a lookup table")]
    AccountNotInTable,
    #[error("duplicate lookup table entry")]
    DuplicateLookupTable,
}

pub type Result<T> = std::result::Result<T, ComposeError>;

/// One leg of a composed cycle, in final-mint order.
#[derive(Debug, Clone)]
pub struct CycleLeg {
    /// Instruction set for this hop, from Jupiter `/swap-instructions`.
    pub instructions: SwapInstructions,
    /// True for the final leg, whose cleanup unwraps WSOL before the guard.
    pub is_final: bool,
}

/// A composed cycle: every leg's swap instructions in one ordered list.
#[derive(Debug, Clone)]
pub struct ComposedCycle {
    pub instructions: Vec<Instruction>,
    /// Sole wallet signature required by the composed transaction.
    /// Always 1: the engine holds exactly one keypair and any instruction
    /// needing a second signer is rejected during composition.
    pub signatures_required: usize,
    /// Address lookup tables the composed transaction references.
    pub lookup_tables: Vec<String>,
    /// Estimated serialized size, conservative (never under-estimates).
    pub estimated_bytes: usize,
}

/// Convert Jupiter's JSON instruction form into a native instruction.
fn to_instruction(instruction: &RawInstruction, index: usize) -> Result<Instruction> {
    let data = instruction
        .data_bytes()
        .map_err(|source| ComposeError::InstructionData { index, source })?;
    let mut accounts = Vec::with_capacity(instruction.accounts.len());
    for account in &instruction.accounts {
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(&account.pubkey.to_string())
            .map_err(|_| ComposeError::BadAccountId { index })?;
        accounts.push(AccountMeta {
            pubkey,
            is_signer: account.is_signer,
            is_writable: account.is_writable,
        });
    }
    let program_id = solana_sdk::pubkey::Pubkey::from_str(&instruction.program_id.to_string())
        .map_err(|_| ComposeError::BadProgramId { index })?;
    Ok(Instruction {
        program_id,
        accounts,
        data,
    })
}

use std::str::FromStr;

/// Controls which Jupiter-provided instructions survive composition.
///
/// Flash cycles need both switches off: the engine sets its own compute budget
/// (the borrow/repay add compute units Jupiter did not quote), and Jupiter's
/// final cleanup must not run because it unwraps/closes the token account that
/// the repayment instruction is about to spend from.
#[derive(Debug, Clone, Copy)]
pub struct ComposeOptions {
    /// Keep the final leg's cleanup instruction (closes/unwraps WSOL).
    pub include_final_cleanup: bool,
    /// Keep Jupiter's compute-budget instructions.
    pub include_compute_budget: bool,
    /// Skip the pre-flight static-key estimate. Use only when the caller adds
    /// an address lookup table after composition: the estimate charges every
    /// key at 32 bytes, while the exact assembled size is still enforced by
    /// `build_composed_transaction` before signing.
    pub skip_size_estimate: bool,
}

impl Default for ComposeOptions {
    fn default() -> Self {
        Self {
            include_final_cleanup: true,
            include_compute_budget: true,
            skip_size_estimate: false,
        }
    }
}

/// Assemble all legs into one ordered instruction list.
///
/// The rules are what make composition safe rather than merely compact:
/// only the first leg contributes a compute-budget preamble (a transaction may
/// set its budget once), and only the last leg contributes a cleanup (earlier
/// cleanups would destroy the accounts the next leg needs).
pub fn compose_cycle(legs: &[CycleLeg]) -> Result<ComposedCycle> {
    compose_cycle_with(legs, ComposeOptions::default(), &[], &[])
}

/// Compose a cycle with engine-owned instructions before and after the swaps.
///
/// `preamble` runs before every Jupiter instruction (used for ATA creation and
/// the flash borrow), and `postamble` runs after the last swap (used for the
/// flash repayment and, for wrapped SOL, closing the account). Both are native
/// instructions, so their account requirements are part of the composed
/// transaction's static key list.
pub fn compose_cycle_with(
    legs: &[CycleLeg],
    options: ComposeOptions,
    preamble: &[Instruction],
    postamble: &[Instruction],
) -> Result<ComposedCycle> {
    if legs.is_empty() {
        return Err(ComposeError::Empty);
    }
    let mut instructions = Vec::new();
    let mut lookup_tables: Vec<String> = Vec::new();
    // Jupiter swap-instructions are signed by the wallet alone.
    let signatures_required = 1usize;

    for (index, leg) in legs.iter().enumerate() {
        if leg.instructions.swap_instruction.is_empty() {
            return Err(ComposeError::MissingSwap { index });
        }
        for table in &leg.instructions.address_lookup_table_addresses {
            let key = table.to_string();
            if !lookup_tables.contains(&key) {
                lookup_tables.push(key);
            }
        }
    }

    for instruction in preamble {
        instructions.push(instruction.clone());
    }

    for (index, leg) in legs.iter().enumerate() {
        // First leg owns the compute budget for the whole transaction.
        // Only the first leg may set the compute budget: a transaction can
        // raise its limit once, and a second budget instruction silently
        // overrides the first, which is worse than not having it.
        let keep_budget = index == 0 && options.include_compute_budget;
        let mut raw_preamble: Vec<&beast_dex::jupiter::RawInstruction> =
            leg.instructions.preamble();
        if !keep_budget {
            let budget_programs: Vec<&beast_dex::Address> = leg
                .instructions
                .compute_budget_instructions
                .iter()
                .map(|budget| &budget.program_id)
                .collect();
            raw_preamble.retain(|instruction| {
                !budget_programs
                    .iter()
                    .any(|program| **program == instruction.program_id)
            });
        }
        for instruction in raw_preamble {
            let instruction = to_instruction(instruction, index)?;
            instructions.push(instruction);
        }

        let swap = to_instruction(&leg.instructions.swap_instruction, index)?;
        if swap.accounts.iter().any(|account| account.is_signer) {
            // The wallet signs the transaction itself; no extra signers are
            // supported because the engine holds exactly one keypair.
            return Err(ComposeError::UnsupportedSigner { index });
        }
        instructions.push(swap);

        for instruction in &leg.instructions.other_instructions {
            let instruction = to_instruction(instruction, index)?;
            instructions.push(instruction);
        }

        // Only the last leg unwraps/closes: earlier legs would delete the
        // token account the following leg spends from.
        if leg.is_final && options.include_final_cleanup {
            if let Some(cleanup) = &leg.instructions.cleanup_instruction {
                let instruction = to_instruction(cleanup, index)?;
                instructions.push(instruction);
            }
        }
    }

    for instruction in postamble {
        instructions.push(instruction.clone());
    }

    let estimated_bytes = estimate_bytes(&instructions);
    tracing::debug!(
        instructions = instructions.len(),
        unique_keys = {
            let mut keys = std::collections::HashSet::new();
            for instruction in &instructions {
                keys.insert(instruction.program_id);
                for meta in &instruction.accounts {
                    keys.insert(meta.pubkey);
                }
            }
            keys.len()
        },
        account_metas = instructions
            .iter()
            .map(|instruction| instruction.accounts.len())
            .sum::<usize>(),
        data_bytes = instructions
            .iter()
            .map(|instruction| instruction.data.len())
            .sum::<usize>(),
        estimated_bytes,
        "composing cycle"
    );
    if !options.skip_size_estimate && estimated_bytes > MAX_TRANSACTION_BYTES {
        return Err(ComposeError::TooLarge {
            needed: estimated_bytes,
        });
    }

    Ok(ComposedCycle {
        instructions,
        signatures_required,
        lookup_tables,
        estimated_bytes,
    })
}

/// Estimate the serialized message size from the actual instruction set.
///
/// The old implementation charged every instruction account a full 32-byte key.
/// That is wildly pessimistic for v0/static-key messages, where repeated keys
/// cost one index byte, and it rejected provably fitting local CPMM cycles. This
/// version charges each unique key once and each instruction meta one index
/// byte. It is still conservative for lookup-table cycles (table-resident keys
/// are charged as static 32-byte keys here) and the exact serialized size is
/// checked again by `build_composed_transaction` before signing.
fn estimate_bytes(instructions: &[Instruction]) -> usize {
    use std::collections::HashSet;
    let mut keys: HashSet<solana_sdk::pubkey::Pubkey> = HashSet::new();
    let mut account_meta_bytes = 0usize;
    let mut data_bytes = 0usize;
    for instruction in instructions {
        keys.insert(instruction.program_id);
        for meta in &instruction.accounts {
            keys.insert(meta.pubkey);
            account_meta_bytes += 1;
        }
        data_bytes += instruction.data.len();
    }
    // signature count + signature + message header + blockhash + key list +
    // instruction count + per-instruction framing (program index, account
    // count, data length) + account indices + instruction data.
    1 + 64 + 3 + 32 + keys.len() * 32 + 1 + instructions.len() * 3 + account_meta_bytes + data_bytes
}

/// Re-serialise a composed transaction to measure its true on-wire size.
///
/// The estimate in `compose_cycle` is deliberately pessimistic; this is the
/// number that decides whether the transaction can actually land.
pub fn exact_bytes(transaction: &Transaction) -> Result<usize> {
    bincode::serialize(transaction)
        .map(|bytes| bytes.len())
        .map_err(|error| ComposeError::Serialize(error.to_string()))
}

/// True when a composed transaction fits Solana's transaction size limit.
pub fn fits(transaction: &Transaction) -> Result<bool> {
    Ok(exact_bytes(transaction)? <= MAX_TRANSACTION_BYTES)
}

/// Base64-encode a composed transaction for Jito.
pub fn encode(transaction: &Transaction) -> Result<String> {
    bincode::serialize(transaction)
        .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
        .map_err(|error| ComposeError::Serialize(error.to_string()))
}

/// An address lookup table fetched from the cluster.
#[derive(Debug, Clone)]
pub struct AddressLookupTable {
    pub key: beast_dex::Address,
    pub addresses: Vec<beast_dex::Address>,
}

/// Convert an on-chain address to a native key.
fn native_key(address: &beast_dex::Address) -> solana_sdk::pubkey::Pubkey {
    solana_sdk::pubkey::Pubkey::new_from_array(address.0)
}

/// Assemble a composed cycle into a **v0** transaction using lookup tables.
///
/// Jupiter returns its instruction sets as v0 transactions for a reason: one
/// WSOL -> USDC hop already references ~25 accounts, and a legacy transaction
/// charges 32 bytes per unique account key, so three composed legs would exceed
/// the 1232-byte limit before the first guard instruction is even added.
///
/// V0 moves most accounts into lookup tables where each costs a single index
/// byte. The cost of that is an exact index encoding. Per the v0 message spec,
/// instruction indexes resolve against the concatenation of three lists:
///
/// 1. static `account_keys`
/// 2. keys loaded by every table's `writable_indexes`, in listed order
/// 3. keys loaded by every table's `readonly_indexes`, in listed order
///
/// A wrong offset produces `InvalidAccountIndex` at runtime, so the offsets here
/// are computed from the same lists that are written into the message.
pub fn assemble_v0(
    composed: &ComposedCycle,
    wallet: solana_sdk::pubkey::Pubkey,
    blockhash: solana_sdk::hash::Hash,
    tables: &[AddressLookupTable],
) -> Result<VersionedTransaction> {
    use std::collections::{BTreeMap, HashMap, HashSet};

    // Two legs may cite the same table; one lookup must cover it. Preserve
    // first-seen order because that is the order the runtime loads addresses.
    let mut unique_tables: Vec<&AddressLookupTable> = Vec::new();
    let mut seen_tables: HashSet<solana_sdk::pubkey::Pubkey> = HashSet::new();
    for table in tables {
        if seen_tables.insert(native_key(&table.key)) {
            unique_tables.push(table);
        }
    }

    // Row lookup for every table-resident account.
    let mut row_of: HashMap<solana_sdk::pubkey::Pubkey, (usize, usize)> = HashMap::new();
    for (table_index, table) in unique_tables.iter().enumerate() {
        for (row, address) in table.addresses.iter().enumerate() {
            row_of
                .entry(native_key(address))
                .or_insert((table_index, row));
        }
    }

    // Every account the cycle touches. A key that is read-only in one
    // instruction and writable in another must be classed writable, otherwise
    // the message is rejected as inconsistent.
    let mut referenced: HashSet<solana_sdk::pubkey::Pubkey> = HashSet::new();
    let mut writable: HashSet<solana_sdk::pubkey::Pubkey> = HashSet::new();
    let mut program_ids: HashSet<solana_sdk::pubkey::Pubkey> = HashSet::new();
    referenced.insert(wallet);
    writable.insert(wallet);
    for instruction in &composed.instructions {
        for meta in &instruction.accounts {
            referenced.insert(meta.pubkey);
            writable.insert(meta.pubkey);
        }
        referenced.insert(instruction.program_id);
        program_ids.insert(instruction.program_id);
    }

    // Static keys: the sole signer first, then writable, then read-only.
    // Program ids must stay static: the runtime rejects a program id loaded
    // from a lookup table.
    let mut static_keys: Vec<solana_sdk::pubkey::Pubkey> = vec![wallet];
    let mut static_seen: HashSet<solana_sdk::pubkey::Pubkey> = HashSet::new();
    static_seen.insert(wallet);
    let mut ordered = referenced.iter().copied().collect::<Vec<_>>();
    ordered.sort_by_key(|address| (!writable.contains(address), *address));
    for address in &ordered {
        let table_resident = row_of.contains_key(address) && !program_ids.contains(address);
        if table_resident || static_seen.contains(address) {
            continue;
        }
        static_seen.insert(*address);
        static_keys.push(*address);
    }
    if static_keys.len() > 64 {
        return Err(ComposeError::TooManyStaticKeys);
    }

    // Group each table's referenced rows into writable and readonly sets.
    // Sort each set by row so position in the list is deterministic; the
    // runtime loads addresses in *list order*, not row order.
    let mut writable_rows: BTreeMap<solana_sdk::pubkey::Pubkey, Vec<u8>> = BTreeMap::new();
    let mut readonly_rows: BTreeMap<solana_sdk::pubkey::Pubkey, Vec<u8>> = BTreeMap::new();
    for address in &referenced {
        if program_ids.contains(address) {
            continue;
        }
        let Some((table_index, row)) = row_of.get(address) else {
            continue;
        };
        let table = unique_tables[*table_index];
        let Ok(row) = u8::try_from(*row) else {
            return Err(ComposeError::TooManyStaticKeys);
        };
        if writable.contains(address) {
            writable_rows
                .entry(native_key(&table.key))
                .or_default()
                .push(row);
        } else {
            readonly_rows
                .entry(native_key(&table.key))
                .or_default()
                .push(row);
        }
    }
    for rows in writable_rows.values_mut().chain(readonly_rows.values_mut()) {
        rows.sort_unstable();
        rows.dedup();
    }

    // Instruction index offsets. The v0 spec concatenates the static keys,
    // then every table's writable addresses in lookup order, then every
    // table's readonly addresses in lookup order. It is *not* writable-then-
    // readonly per table, which is only equivalent with a single table.
    let mut writable_offset: HashMap<solana_sdk::pubkey::Pubkey, usize> = HashMap::new();
    let mut readonly_offset: HashMap<solana_sdk::pubkey::Pubkey, usize> = HashMap::new();
    let mut cursor = static_keys.len();
    for table in &unique_tables {
        let native = native_key(&table.key);
        let len = writable_rows.get(&native).map(Vec::len).unwrap_or(0);
        if len > 0 {
            writable_offset.insert(native, cursor);
            cursor += len;
        }
    }
    for table in &unique_tables {
        let native = native_key(&table.key);
        let len = readonly_rows.get(&native).map(Vec::len).unwrap_or(0);
        if len > 0 {
            readonly_offset.insert(native, cursor);
            cursor += len;
        }
    }

    let static_position: HashMap<solana_sdk::pubkey::Pubkey, usize> = static_keys
        .iter()
        .enumerate()
        .map(|(index, key)| (*key, index))
        .collect();
    let index_for = |address: solana_sdk::pubkey::Pubkey| -> Result<u8> {
        if let Some(index) = static_position.get(&address) {
            return u8::try_from(*index).map_err(|_| ComposeError::TooManyStaticKeys);
        }
        let (table_index, row) = row_of
            .get(&address)
            .copied()
            .ok_or(ComposeError::AccountNotInTable)?;
        let table = unique_tables[table_index];
        let native = native_key(&table.key);
        let row = u8::try_from(row).map_err(|_| ComposeError::TooManyStaticKeys)?;
        let (offset, rows) = if writable.contains(&address) {
            (writable_offset.get(&native), writable_rows.get(&native))
        } else {
            (readonly_offset.get(&native), readonly_rows.get(&native))
        };
        let offset = offset.ok_or(ComposeError::AccountNotInTable)?;
        let rows = rows.ok_or(ComposeError::AccountNotInTable)?;
        let position = rows
            .binary_search(&row)
            .map_err(|_| ComposeError::AccountNotInTable)?;
        u8::try_from(offset + position).map_err(|_| ComposeError::TooManyStaticKeys)
    };

    let instructions = composed
        .instructions
        .iter()
        .map(|instruction| {
            Ok(CompiledInstruction {
                program_id_index: index_for(instruction.program_id)?,
                accounts: instruction
                    .accounts
                    .iter()
                    .map(|meta| index_for(meta.pubkey))
                    .collect::<Result<Vec<u8>>>()?,
                data: instruction.data.clone(),
            })
        })
        .collect::<Result<Vec<CompiledInstruction>>>()?;

    let address_table_lookups = unique_tables
        .iter()
        .filter_map(|table| {
            let native = native_key(&table.key);
            let writable_indexes = writable_rows.get(&native).cloned().unwrap_or_default();
            let readonly_indexes = readonly_rows.get(&native).cloned().unwrap_or_default();
            // The v0 sanitizer rejects an empty lookup, so omit tables this
            // cycle does not actually reference.
            if writable_indexes.is_empty() && readonly_indexes.is_empty() {
                return None;
            }
            Some(MessageAddressTableLookup {
                account_key: native,
                writable_indexes,
                readonly_indexes,
            })
        })
        .collect::<Vec<_>>();

    let message = VersionedMessage::V0(V0Message {
        header: MessageHeader {
            num_required_signatures: 1,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: static_keys
                .iter()
                .filter(|key| !writable.contains(*key))
                .count() as u8,
        },
        account_keys: static_keys,
        recent_blockhash: blockhash,
        instructions,
        address_table_lookups,
    });
    Ok(VersionedTransaction {
        signatures: vec![solana_sdk::signature::Signature::default()],
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use beast_dex::jupiter::RawAccountMeta;

    const BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";

    /// Deterministic, valid pubkeys so instruction fixtures round-trip.
    fn pubkey(seed: u8) -> beast_dex::Address {
        let mut bytes = [0u8; 32];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = (seed.wrapping_add(index as u8)) % 250;
        }
        beast_dex::Address::from_slice(&bytes).expect("32 bytes")
    }

    fn instruction(label: u8, accounts: usize) -> RawInstruction {
        RawInstruction {
            program_id: pubkey(label),
            accounts: (0..accounts)
                .map(|index| RawAccountMeta {
                    pubkey: pubkey((index as u8).wrapping_add(100)),
                    is_signer: false,
                    is_writable: index % 2 == 0,
                })
                .collect(),
            data: base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3, 4]),
        }
    }

    fn budget() -> RawInstruction {
        RawInstruction {
            program_id: BUDGET_PROGRAM.parse().unwrap(),
            accounts: Vec::new(),
            data: base64::engine::general_purpose::STANDARD.encode([0u8; 8]),
        }
    }

    fn legs(count: usize) -> Vec<CycleLeg> {
        (0..count)
            .map(|index| CycleLeg {
                is_final: index + 1 == count,
                instructions: SwapInstructions {
                    token_ledger_instruction: None,
                    compute_budget_instructions: vec![budget()],
                    setup_instructions: vec![instruction(index as u8, 2)],
                    swap_instruction: instruction((index as u8).wrapping_add(30), 6),
                    cleanup_instruction: Some(instruction((index as u8).wrapping_add(60), 3)),
                    other_instructions: Vec::new(),
                    address_lookup_table_addresses: vec![],
                    prioritization_fee_lamports: serde_json::Value::Null,
                    compute_unit_limit: None,
                    transaction_version: serde_json::Value::Null,
                    simulation_slot: None,
                    dynamic_slippage_report: None,
                    simulation_error: serde_json::Value::Null,
                    blockhash_with_metadata: None,
                },
            })
            .collect()
    }

    #[test]
    fn composes_every_leg_into_one_instruction_list() {
        let composed = compose_cycle(&legs(3)).expect("3 legs compose");
        // Leg 1: budget + setup + swap. Legs 2-3: setup + swap. Final cleanup.
        assert_eq!(composed.instructions.len(), 3 + 2 + 2 + 1);
    }

    #[test]
    fn keeps_only_one_compute_budget_preamble() {
        let composed = compose_cycle(&legs(3)).expect("3 legs compose");
        let budgets = composed
            .instructions
            .iter()
            .filter(|instruction| instruction.program_id.to_string() == BUDGET_PROGRAM)
            .count();
        assert_eq!(budgets, 1, "a transaction may set its compute budget once");
    }

    #[test]
    fn drops_intermediate_cleanups() {
        let cycle = legs(3);
        let composed = compose_cycle(&cycle).expect("3 legs compose");
        let cleanup_programs: std::collections::HashSet<String> = cycle
            .iter()
            .filter_map(|leg| {
                leg.instructions
                    .cleanup_instruction
                    .as_ref()
                    .map(|instruction| instruction.program_id.to_string())
            })
            .collect();
        let final_cleanup = cycle
            .last()
            .unwrap()
            .instructions
            .cleanup_instruction
            .as_ref()
            .unwrap()
            .program_id
            .to_string();
        let cleanups = composed
            .instructions
            .iter()
            .filter(|instruction| cleanup_programs.contains(&instruction.program_id.to_string()))
            .count();
        assert_eq!(cleanups, 1, "only the final leg may clean up");
        assert!(composed
            .instructions
            .iter()
            .any(|instruction| instruction.program_id.to_string() == final_cleanup));
    }

    #[test]
    fn flash_hooks_wrap_every_jupiter_instruction() {
        use super::native_key;
        let borrow = solana_sdk::instruction::Instruction {
            program_id: native_key(&pubkey(200)),
            accounts: Vec::new(),
            data: vec![1, 2, 3],
        };
        let repay = solana_sdk::instruction::Instruction {
            program_id: native_key(&pubkey(201)),
            accounts: Vec::new(),
            data: vec![4, 5, 6],
        };
        let composed = compose_cycle_with(
            &legs(2),
            ComposeOptions {
                include_final_cleanup: false,
                include_compute_budget: false,
                skip_size_estimate: false,
            },
            std::slice::from_ref(&borrow),
            std::slice::from_ref(&repay),
        )
        .expect("flash cycle composes");
        assert_eq!(composed.instructions.first().unwrap().data, vec![1, 2, 3]);
        assert_eq!(composed.instructions.last().unwrap().data, vec![4, 5, 6]);
    }

    #[test]
    fn flash_options_drop_budget_and_cleanup() {
        let composed = compose_cycle_with(
            &legs(2),
            ComposeOptions {
                include_final_cleanup: false,
                include_compute_budget: false,
                skip_size_estimate: false,
            },
            &[],
            &[],
        )
        .expect("flash cycle composes");
        assert!(!composed
            .instructions
            .iter()
            .any(|instruction| instruction.program_id.to_string() == BUDGET_PROGRAM));
        let cleanups: std::collections::HashSet<String> = legs(2)
            .iter()
            .filter_map(|leg| {
                leg.instructions
                    .cleanup_instruction
                    .as_ref()
                    .map(|instruction| instruction.program_id.to_string())
            })
            .collect();
        assert!(!composed
            .instructions
            .iter()
            .any(|instruction| cleanups.contains(&instruction.program_id.to_string())));
    }

    #[test]
    fn rejects_a_leg_with_no_swap_instruction() {
        let mut malformed = legs(2);
        malformed[0].instructions.swap_instruction = RawInstruction {
            program_id: pubkey(1),
            accounts: Vec::new(),
            data: String::new(),
        };
        let error = compose_cycle(&malformed).unwrap_err();
        assert!(matches!(error, ComposeError::MissingSwap { index: 0 }));
    }

    #[test]
    fn rejects_an_empty_leg_list() {
        let error = compose_cycle(&[]).unwrap_err();
        assert!(matches!(error, ComposeError::Empty));
    }

    #[test]
    fn rejects_an_instruction_needing_a_second_signer() {
        let mut malformed = legs(2);
        malformed[0]
            .instructions
            .swap_instruction
            .accounts
            .push(RawAccountMeta {
                pubkey: pubkey(9),
                is_signer: true,
                is_writable: false,
            });
        let error = compose_cycle(&malformed).unwrap_err();
        assert!(matches!(
            error,
            ComposeError::UnsupportedSigner { index: 0 }
        ));
    }

    #[test]
    fn the_size_estimate_grows_with_leg_count() {
        let two = compose_cycle(&legs(2)).expect("2 legs compose");
        let four = compose_cycle(&legs(4)).expect("4 compact legs compose");
        assert!(two.estimated_bytes < four.estimated_bytes);
        assert!(four.estimated_bytes <= MAX_TRANSACTION_BYTES);
    }

    #[test]
    fn rejects_an_oversized_instruction_set() {
        use super::native_key;
        let oversized = solana_sdk::instruction::Instruction {
            program_id: native_key(&pubkey(1)),
            accounts: Vec::new(),
            data: vec![0u8; MAX_TRANSACTION_BYTES],
        };
        let error = compose_cycle_with(
            &legs(2),
            ComposeOptions::default(),
            std::slice::from_ref(&oversized),
            &[],
        )
        .unwrap_err();
        assert!(matches!(error, ComposeError::TooLarge { .. }));
    }

    #[test]
    fn assembles_a_v0_transaction_with_lookup_tables() {
        use super::native_key;
        let composed = compose_cycle(&legs(2)).expect("2 legs compose");
        let wallet = native_key(&pubkey(200));
        let blockhash = solana_sdk::hash::Hash::new_unique();
        // Every account referenced by the fixtures lives in one table.
        let mut addresses = vec![wallet];
        for instruction in &composed.instructions {
            for meta in &instruction.accounts {
                if !addresses.contains(&meta.pubkey) {
                    addresses.push(meta.pubkey);
                }
            }
            if !addresses.contains(&instruction.program_id) {
                addresses.push(instruction.program_id);
            }
        }
        let table = AddressLookupTable {
            key: pubkey(250),
            addresses: addresses
                .iter()
                .map(|key| beast_dex::Address::from_slice(key.as_ref()).expect("32-byte key"))
                .collect(),
        };
        let transaction =
            assemble_v0(&composed, wallet, blockhash, &[table]).expect("v0 assembly succeeds");
        assert_eq!(transaction.signatures.len(), 1);
        let solana_sdk::message::VersionedMessage::V0(message) = &transaction.message else {
            panic!("expected a v0 message");
        };
        // Program ids are always static (the runtime forbids loading them from
        // a table); non-program accounts live in the table.
        assert_eq!(message.account_keys[0], wallet);
        for instruction in &composed.instructions {
            assert!(
                message.account_keys.contains(&instruction.program_id),
                "program ids must stay static"
            );
        }
        assert_eq!(message.header.num_required_signatures, 1);
        assert_eq!(message.address_table_lookups.len(), 1);
        assert!(!message.address_table_lookups[0].writable_indexes.is_empty());
        assert_eq!(message.instructions.len(), composed.instructions.len());
    }

    #[test]
    fn v0_indices_stay_in_range() {
        // Every instruction index must resolve inside the concatenated
        // static + writable-table + readonly-table key space.
        let composed = compose_cycle(&legs(3)).expect("3 legs compose");
        let mut table_addresses = vec![pubkey(210)];
        for instruction in &composed.instructions {
            for meta in &instruction.accounts {
                let address =
                    beast_dex::Address::from_slice(meta.pubkey.as_ref()).expect("32-byte pubkey");
                if !table_addresses.contains(&address) {
                    table_addresses.push(address);
                }
            }
        }
        let table = AddressLookupTable {
            key: pubkey(220),
            addresses: table_addresses,
        };
        let wallet = native_key(&pubkey(210));
        let transaction = assemble_v0(
            &composed,
            wallet,
            solana_sdk::hash::Hash::new_unique(),
            &[table],
        )
        .expect("v0 assembly succeeds");
        let solana_sdk::message::VersionedMessage::V0(message) = &transaction.message else {
            panic!("expected a v0 message");
        };
        let total = message.account_keys.len()
            + message
                .address_table_lookups
                .iter()
                .map(|lookup| lookup.writable_indexes.len() + lookup.readonly_indexes.len())
                .sum::<usize>();
        for instruction in &message.instructions {
            assert!(
                (instruction.program_id_index as usize) < total,
                "program index {} out of range {}",
                instruction.program_id_index,
                total
            );
            for index in &instruction.accounts {
                assert!(
                    (*index as usize) < total,
                    "account index {index} out of range {total}"
                );
            }
        }
    }

    #[test]
    fn v0_lookup_indices_resolve_to_the_original_accounts() {
        use super::native_key;

        // A v0 lookup consumes addresses in *list order*. Referencing table
        // rows out of order used to encode `offset + row` while writing a
        // hash-ordered list, so the program received a different account.
        fn resolve(
            message: &solana_sdk::message::v0::Message,
            tables: &[AddressLookupTable],
        ) -> Vec<solana_sdk::pubkey::Pubkey> {
            let mut keys = message.account_keys.clone();
            let by_key: std::collections::HashMap<_, _> = tables
                .iter()
                .map(|table| (native_key(&table.key), table))
                .collect();
            for lookup in &message.address_table_lookups {
                let table = by_key[&lookup.account_key];
                for index in &lookup.writable_indexes {
                    keys.push(native_key(&table.addresses[*index as usize]));
                }
            }
            for lookup in &message.address_table_lookups {
                let table = by_key[&lookup.account_key];
                for index in &lookup.readonly_indexes {
                    keys.push(native_key(&table.addresses[*index as usize]));
                }
            }
            keys
        }

        let wallet = native_key(&pubkey(200));
        let program = native_key(&pubkey(201));
        let address = |seed: u8| native_key(&pubkey(seed));
        let table_a = AddressLookupTable {
            key: pubkey(250),
            addresses: (10u8..16).map(pubkey).collect(),
        };
        let table_b = AddressLookupTable {
            key: pubkey(249),
            addresses: (20u8..23).map(pubkey).collect(),
        };
        let call = |accounts: Vec<solana_sdk::pubkey::Pubkey>| Instruction {
            program_id: program,
            accounts: accounts
                .into_iter()
                .map(|pubkey| AccountMeta {
                    pubkey,
                    is_signer: false,
                    is_writable: true,
                })
                .collect(),
            data: vec![1],
        };
        // Deliberately non-monotonic rows, and a second table whose rows start
        // after the first table's writable list.
        let composed = ComposedCycle {
            instructions: vec![
                call(vec![address(15)]),
                call(vec![address(12)]),
                call(vec![address(10)]),
                call(vec![address(22)]),
                call(vec![address(13)]),
            ],
            signatures_required: 1,
            lookup_tables: vec![table_a.key.to_string(), table_b.key.to_string()],
            estimated_bytes: 0,
        };
        let transaction = assemble_v0(
            &composed,
            wallet,
            solana_sdk::hash::Hash::new_unique(),
            &[table_b.clone(), table_a.clone()],
        )
        .expect("v0 assembly succeeds");
        let solana_sdk::message::VersionedMessage::V0(message) = &transaction.message else {
            panic!("expected a v0 message");
        };
        let resolved = resolve(message, &[table_b, table_a]);
        for (instruction, compiled) in composed.instructions.iter().zip(&message.instructions) {
            assert_eq!(
                resolved[compiled.program_id_index as usize], instruction.program_id,
                "program index must resolve to the program id"
            );
            for (meta, index) in instruction.accounts.iter().zip(&compiled.accounts) {
                assert_eq!(
                    resolved[*index as usize], meta.pubkey,
                    "account index must resolve to the referenced account"
                );
            }
        }
    }

    #[test]
    fn decodes_base64_instruction_payloads() {
        assert_eq!(instruction(3, 1).data_bytes().unwrap(), vec![1, 2, 3, 4]);
    }
}
