use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Error, Debug, Copy, Clone)]
pub enum EscrowError {
    #[error("invalid Instruction")]
    InvalidInstruction,

    #[error("Not rent exempt, add some amount into the account")]
    NotRentExempt,

    #[error("Expected amount is mismatched")]
    ExpectedAmountMismatch,

    #[error("Amount overflow")]
    AmountOverflow,
}

impl From<EscrowError> for ProgramError {
    fn from(e: EscrowError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
