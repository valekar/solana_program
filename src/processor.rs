//use solana_program::program_pack::Pack;
use solana_program::program_pack::{IsInitialized, Pack};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{rent::Rent, Sysvar},
};
use spl_token;

use crate::{error::EscrowError, instruction::EscrowInstruction, state::Escrow};
use spl_token::state::Account as TokenAccount;

pub struct Processor;

impl Processor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        let instruction = EscrowInstruction::unpack(instruction_data)?;

        match instruction {
            EscrowInstruction::InitEscrow { amount } => {
                msg!("Instruction : Init Escrow");

                Self::process_init_escrow(accounts, amount, program_id)
            }

            EscrowInstruction::Exchange { amount } => {
                msg!("instruction : Exchange escrow");

                Self::process_exchange(accounts, amount, program_id)
            }
        }
    }

    fn process_init_escrow(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let initializer = next_account_info(account_info_iter)?;

        //check if he is a valid owner
        if !initializer.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        // used to pass ownership of token account of initializer
        let temp_token_account = next_account_info(account_info_iter)?;

        // from another party to initiliazer's token account
        let token_to_receive_account = next_account_info(account_info_iter)?;

        // checking if token account is owned by program ( this is solana attribute)
        if *token_to_receive_account.owner != spl_token::id() {
            return Err(ProgramError::IncorrectProgramId);
        }

        // escrow program's account
        let escrow_account = next_account_info(account_info_iter)?;

        // checking if escrow's account has enough money in it
        let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;

        // enough money check
        if !rent.is_exempt(escrow_account.lamports(), escrow_account.data_len()) {
            return Err(EscrowError::NotRentExempt.into());
        }

        // get info of escrow program (state)
        let mut escrow_info: Escrow = Escrow::unpack_unchecked(&escrow_account.data.borrow())?;

        // check if the escrow is already in use
        if escrow_info.is_initialized() {
            return Err(ProgramError::AccountAlreadyInitialized);
        }

        escrow_info.is_initialized = true;
        escrow_info.initializer_pubkey = *initializer.key;
        escrow_info.temp_token_account_pubkey = *temp_token_account.key;
        escrow_info.initializer_token_to_receive_account_pubkey = *token_to_receive_account.key;
        escrow_info.expected_amount = amount;

        Escrow::pack(escrow_info, &mut escrow_account.data.borrow_mut())?;

        let (pda, _bump_seed) = Pubkey::find_program_address(&[b"escrow"], program_id);

        let token_program = next_account_info(account_info_iter)?;

        let owner_change_ix = spl_token::instruction::set_authority(
            token_program.key,
            temp_token_account.key, // this account will get PDA
            Some(&pda),
            spl_token::instruction::AuthorityType::AccountOwner,
            initializer.key,
            &[&initializer.key],
        )?;

        msg!("calling the otken program to transfer token account ownership...");

        // calling this so that temp token account gets pda
        invoke(
            &owner_change_ix,
            &[
                temp_token_account.clone(),
                initializer.clone(),
                token_program.clone(),
            ],
        )?;

        Ok(())
    }

    fn process_exchange(
        accounts: &[AccountInfo],
        amount_expected_by_taker: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let taker = next_account_info(account_info_iter)?;

        if !taker.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let takers_sending_token_account = next_account_info(account_info_iter)?;

        let takers_token_to_receive_account = next_account_info(account_info_iter)?;

        let pdas_temp_token_account = next_account_info(account_info_iter)?;

        let pdas_temp_token_account_info =
            TokenAccount::unpack(&pdas_temp_token_account.data.borrow())?;

        let (pda, bump_seed) = Pubkey::find_program_address(&[b"escrow"], program_id);

        if amount_expected_by_taker != pdas_temp_token_account_info.amount {
            return Err(EscrowError::ExpectedAmountMismatch.into());
        }

        let initializer_main_account = next_account_info(account_info_iter)?;

        let initializers_token_to_receive_account = next_account_info(account_info_iter)?;

        let escrow_account = next_account_info(account_info_iter)?;

        let escrow_info = Escrow::unpack(&escrow_account.data.borrow())?;

        if escrow_info.temp_token_account_pubkey != *pdas_temp_token_account.key {
            return Err(ProgramError::InvalidAccountData);
        }

        if escrow_info.initializer_pubkey != *initializer_main_account.key {
            return Err(ProgramError::InvalidAccountData);
        }

        if escrow_info.initializer_token_to_receive_account_pubkey
            != *initializers_token_to_receive_account.key
        {
            return Err(ProgramError::InvalidAccountData);
        }

        let token_program = next_account_info(account_info_iter)?;

        let transfer_to_initializer_ix = spl_token::instruction::transfer(
            token_program.key,
            takers_sending_token_account.key,
            initializers_token_to_receive_account.key,
            taker.key,
            &[&taker.key],
            escrow_info.expected_amount,
        )?;

        msg!("Calling the token program to transfer tokens to the escrow's initializer ....");

        invoke(
            &transfer_to_initializer_ix,
            &[
                takers_sending_token_account.clone(),
                initializers_token_to_receive_account.clone(),
                taker.clone(),
                token_program.clone(),
            ],
        )?;

        let pda_account = next_account_info(account_info_iter)?;

        let transfer_to_taker_ix = spl_token::instruction::transfer(
            token_program.key,
            pdas_temp_token_account.key,
            takers_token_to_receive_account.key,
            &pda,
            &[&pda],
            pdas_temp_token_account_info.amount,
        )?;

        msg!("calling the token program to transfer tokens to the taker ...");

        invoke_signed(
            &transfer_to_taker_ix,
            &[
                pdas_temp_token_account.clone(),
                takers_token_to_receive_account.clone(),
                pda_account.clone(),
                token_program.clone(),
            ],
            &[&[&b"escrow"[..], &[bump_seed]]],
        )?;

        msg!("calling the otken program to close pda's temp account ...");
        let close_pdas_temp_acc_ix = spl_token::instruction::close_account(
            token_program.key,
            pdas_temp_token_account.key,
            initializer_main_account.key,
            &pda,
            &[&pda],
        )?;

        invoke_signed(
            &close_pdas_temp_acc_ix,
            &[
                pdas_temp_token_account.clone(),
                initializer_main_account.clone(),
                pda_account.clone(),
                token_program.clone(),
            ],
            &[&[&b"escrow"[..], &[bump_seed]]],
        )?;

        msg!("closing the escrow account...");

        **initializer_main_account.lamports.borrow_mut() = initializer_main_account
            .lamports()
            .checked_add(escrow_account.lamports())
            .ok_or(EscrowError::AmountOverflow)?;

        **escrow_account.lamports.borrow_mut() = 0;

        Ok(())
    }
}
