//! SOL BEAST on-chain profit guard.
//!
//! The bundle places this instruction AFTER the swaps and BEFORE the Jito tip
//! transfer. The guard reads the wallet's live lamport balance and aborts the
//! bundle unless:
//!
//! `final_lamports >= expected_start + min_profit + tip + fees`
//!
//! `fees` is the cost still to be paid *after* this instruction (the tip
//! transaction's fee). Fees and ATA rent already paid by the preceding swap
//! transactions are reflected in `final_lamports`, so they must not be
//! subtracted a second time.
//!
//! The caller supplies `expected_start` from an RPC balance immediately before
//! building the bundle. The transaction is signed by the wallet, so no third
//! party can alter the threshold. The guard is intentionally tiny and has no
//! admin, upgrade authority assumptions, or token accounts.

// `solana_program`'s entrypoint macro probes legacy cfg values (`solana`,
// `custom-heap`, `custom-panic`) that are not declared as Cargo features here.
// They are harmless on the host and required on-chain.
#![allow(unexpected_cfgs)]

use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, msg,
    program_error::ProgramError, pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};

pub const IX_VERSION: u8 = 1;
pub const IX_LEN: usize = 33;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardArgs {
    pub expected_start: u64,
    pub min_profit: u64,
    pub tip: u64,
    pub fees: u64,
}

impl GuardArgs {
    pub fn required_final(self) -> Option<u64> {
        self.expected_start
            .checked_add(self.min_profit)?
            .checked_add(self.tip)?
            .checked_add(self.fees)
    }
}

pub fn parse_args(data: &[u8]) -> Result<GuardArgs, ProgramError> {
    if data.len() != IX_LEN || data[0] != IX_VERSION {
        return Err(ProgramError::InvalidInstructionData);
    }
    let read = |offset: usize| -> u64 {
        u64::from_le_bytes(
            data[offset..offset + 8]
                .try_into()
                .expect("IX_LEN guarantees 8-byte fields"),
        )
    };
    Ok(GuardArgs {
        expected_start: read(1),
        min_profit: read(9),
        tip: read(17),
        fees: read(25),
    })
}

entrypoint!(process_instruction);

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let wallet = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    if !wallet.is_signer {
        msg!("BEAST guard: wallet must sign");
        return Err(ProgramError::MissingRequiredSignature);
    }
    if !wallet.is_writable {
        msg!("BEAST guard: wallet must be writable");
        return Err(ProgramError::InvalidAccountData);
    }
    // The system program is the all-zero pubkey. Comparing to `Pubkey::default`
    // avoids the deprecated `system_program` re-export in solana-program 2.x.
    if wallet.owner != &Pubkey::default() {
        msg!("BEAST guard: wallet must be a system account");
        return Err(ProgramError::InvalidAccountOwner);
    }
    let args = parse_args(data)?;
    let required = args
        .required_final()
        .ok_or(ProgramError::InvalidInstructionData)?;
    if args.expected_start == 0 {
        msg!("BEAST guard: zero expected_start");
        return Err(ProgramError::InvalidInstructionData);
    }
    let final_lamports = wallet.lamports();
    let rent_minimum = Rent::get()?.minimum_balance(wallet.data_len());
    if final_lamports < required {
        msg!(
            "BEAST guard: final {} < required {} (start {} profit {} tip {} fees {})",
            final_lamports,
            required,
            args.expected_start,
            args.min_profit,
            args.tip,
            args.fees
        );
        return Err(ProgramError::Custom(1));
    }
    if final_lamports < rent_minimum {
        msg!(
            "BEAST guard: final {} below rent minimum {}",
            final_lamports,
            rent_minimum
        );
        return Err(ProgramError::Custom(2));
    }
    msg!(
        "BEAST guard ok: final {} >= required {}",
        final_lamports,
        required
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(start: u64, profit: u64, tip: u64, fees: u64) -> Vec<u8> {
        let mut data = vec![IX_VERSION];
        data.extend_from_slice(&start.to_le_bytes());
        data.extend_from_slice(&profit.to_le_bytes());
        data.extend_from_slice(&tip.to_le_bytes());
        data.extend_from_slice(&fees.to_le_bytes());
        data
    }

    #[test]
    fn parses_exact_layout() {
        let data = args(1_000, 20, 10, 5);
        assert_eq!(
            parse_args(&data).unwrap(),
            GuardArgs {
                expected_start: 1_000,
                min_profit: 20,
                tip: 10,
                fees: 5
            }
        );
        assert_eq!(data.len(), IX_LEN);
    }

    #[test]
    fn rejects_wrong_version_or_length() {
        assert!(parse_args(&[0u8; IX_LEN]).is_err());
        assert!(parse_args(&[IX_VERSION; IX_LEN - 1]).is_err());
    }

    #[test]
    fn overflow_is_rejected() {
        let data = args(u64::MAX, 1, 1, 1);
        let parsed = parse_args(&data).unwrap();
        assert!(parsed.required_final().is_none());
    }

    #[test]
    fn required_final_saturates_nothing() {
        let data = args(100, 20, 10, 5);
        assert_eq!(parse_args(&data).unwrap().required_final(), Some(135));
    }
}
