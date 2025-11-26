use crate::core::models::Protocol;
use solana_program::pubkey::Pubkey;

#[allow(dead_code)]
pub struct PumpfunProtocol {
    program_id: Pubkey,
}

#[allow(dead_code)]
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