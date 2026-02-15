# Pump.fun IDL Files

This directory contains the official IDL (Interface Definition Language) files for pump.fun program interactions.

## Files

- **pumpfun.json**: Official pump.fun program IDL for buy/sell operations
  - Program ID: `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`
  - Contains instruction definitions for `buy` and `sell` operations
  - Includes complete account metadata and PDA seed derivations

- **pumpfunfees.json**: Pump.fun fee program IDL
  - Program ID: `pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ`
  - Contains fee configuration, fee tiers, and fee calculation instructions
  - Used for fee_config PDA derivation and fee recipient validation

- **pumpfunamm.json**: Pump.fun AMM / PumpSwap program IDL
  - Program ID: `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`
  - Contains `buy_token_sol` and `sell_token_sol` instructions for post-migration token trading
  - Used when tokens have graduated from bonding curves to AMM pools

## Source

These IDL files are structured to match the official pump.fun program interface. The IDL format follows the Anchor framework standard with:

- Instruction discriminators (8-byte identifiers derived from instruction names)
- Account definitions with mutability and signer flags
- PDA (Program Derived Address) seed specifications
- Argument types for each instruction

## Usage

The IDL files are automatically loaded by the application at startup via `load_all_idls()` in `src/idl.rs`. The transaction builder (`src/tx_builder.rs`) uses these IDLs to construct buy and sell instructions with the correct account ordering and metadata.

## Structure

The IDL format includes:
- `version`: IDL version
- `name`: Program name
- `address`: Program public key
- `instructions`: Array of instruction definitions
  - `name`: Instruction name
  - `discriminator`: 8-byte instruction identifier
  - `accounts`: Array of account requirements with PDA specifications
  - `args`: Instruction arguments

## Notes

- The IDL loader prioritizes bundled files in this directory over legacy root-level files
- If IDL files are not found, the loader returns no IDLs; any on-chain IDL fetching must be performed explicitly by the caller
- All transactions should be built using these official IDL definitions to ensure compatibility
