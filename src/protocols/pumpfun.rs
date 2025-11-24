use crate::protocols::Protocol;
use solana_program::pubkey::Pubkey;

pub struct PumpfunProtocol {
    program_id: Pubkey,
}

impl PumpfunProtocol {
    pub fn new(program_id: Pubkey) -> Self {
        PumpfunProtocol { program_id }
    }
}

impl Protocol for PumpfunProtocol {
    fn program_id(&self) -> Pubkey {
        self.program_id
    }

    fn name(&self) -> String {
        "pump.fun".to_string()
    }
}
