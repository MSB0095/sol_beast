import * as borsh from 'borsh'

// Re-export named functions expected by @solana/web3.js
export const serialize = borsh.serialize
export const deserialize = borsh.deserialize
export const deserializeUnchecked = borsh.deserializeUnchecked

export default borsh
