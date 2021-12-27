use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use std::convert::Into;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[error]
pub enum ErrorCode {
    #[msg("The given owner is not part of this multisig.")]
    InvalidOwner,
    #[msg("Not enough owners signed this transaction.")]
    NotEnoughSigners,
    #[msg("Cannot delete a transaction that has been signed by an owner.")]
    TransactionAlreadySigned,
    #[msg("Overflow when adding.")]
    Overflow,
    #[msg("Cannot delete a transaction the owner did not create.")]
    UnableToDelete,
    #[msg("The given transaction has already been executed.")]
    AlreadyExecuted,
    #[msg("Threshold must be less than or equal to the number of owners.")]
    InvalidThreshold,
    #[msg("Delay must be less than 30 days.")]
    InvalidDelay,
    #[msg("Owners changed.")]
    OwnersChanged,
    #[msg("Before transation ETA.")]
    BeforeETA,
    #[msg("Unique Owners.")]
    UniqueOwners,
}

#[account]
pub struct Multisig {
    pub base: Pubkey,
    pub bump: u8,
    pub threshold: u64,
    pub delay: i64,
    pub grace_period: i64,
    pub num_transactions: u64,
    pub owners_seq_no: u64,
    pub owners: Vec<Pubkey>,
    _reserved: [u64; 16],
}

#[account]
pub struct Transaction {
    pub multisig: Pubkey,
    pub index: u64,
    pub bump: u8,
    pub eta: i64,
    pub owners_seq_no: u64,
    pub proposer: Pubkey,
    pub instructions: Vec<TransactionInstruction>,
    pub signers: Vec<bool>,
    pub executor: Pubkey,
    pub executed_at: i64,
    _reserved: [u64; 16],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default, PartialEq)]
pub struct TransactionInstruction {
    pub program_id: Pubkey,
    pub keys: Vec<TransactionInstructionMeta>,
    pub data: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, PartialEq, Copy, Clone)]
pub struct TransactionInstructionMeta {
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
}

#[program]
pub mod multisig {
    use super::*;

    #[derive(Accounts)]
    #[instruction(owners: Vec<Pubkey>, threshold: u64, delay: i64, bump: u8)]
    pub struct CreateMultisig<'info> {
        #[account(mut)]
        pub signer: Signer<'info>,
        pub base: AccountInfo<'info>,
        #[account(
            init,
            seeds = [
                b"multisig",
                base.key().to_bytes().as_ref()
            ],
            bump = bump,
            payer = signer,
            space = 4 + std::mem::size_of::<Multisig>() + 4 + (15*32),
        )]
        multisig: Account<'info, Multisig>,
        system_program: Program<'info, System>,
    }

    pub fn create_multisig(
        ctx: Context<CreateMultisig>,
        owners: Vec<Pubkey>,
        threshold: u64,
        delay: i64,
        bump: u8,
    ) -> ProgramResult {
        let multisig = &mut ctx.accounts.multisig;
        require_unique_owners(&owners)?;
        multisig.base = ctx.accounts.base.key();
        multisig.bump = bump;
        multisig.threshold = threshold;
        multisig.delay = delay;
        multisig.grace_period = 14 * 24 * 3600;
        multisig.owners = owners;
        Ok(())
    }

    #[derive(Accounts)]
    pub struct SetOwners<'info> {
        #[account(mut, signer)]
        multisig: Account<'info, Multisig>,
    }

    pub fn set_owners(ctx: Context<SetOwners>, owners: Vec<Pubkey>) -> ProgramResult {
        let multisig = &mut ctx.accounts.multisig;
        require_unique_owners(&owners)?;
        if (owners.len() as u64) < multisig.threshold {
            multisig.threshold = owners.len() as u64;
        }
        multisig.owners = owners;
        multisig.owners_seq_no = multisig
            .owners_seq_no
            .checked_add(1)
            .ok_or(ErrorCode::Overflow)?;
        Ok(())
    }

    #[derive(Accounts)]
    pub struct ChangeThreshold<'info> {
        #[account(mut, signer)]
        multisig: Account<'info, Multisig>,
    }

    pub fn change_threshold(ctx: Context<ChangeThreshold>, threshold: u64) -> ProgramResult {
        let multisig = &mut ctx.accounts.multisig;
        if threshold > multisig.owners.len() as u64 {
            return Err(ErrorCode::InvalidThreshold.into());
        }
        multisig.threshold = threshold;
        Ok(())
    }

    #[derive(Accounts)]
    pub struct ChangeDelay<'info> {
        #[account(mut, signer)]
        multisig: Account<'info, Multisig>,
    }

    pub fn change_delay(ctx: Context<ChangeDelay>, delay: i64) -> ProgramResult {
        let multisig = &mut ctx.accounts.multisig;
        if delay > 30 * 24 * 3600 {
            return Err(ErrorCode::InvalidDelay.into());
        }
        multisig.delay = delay;
        Ok(())
    }

    #[derive(Accounts)]
    #[instruction(instructions: Vec<TransactionInstruction>, bump: u8)]
    pub struct CreateTransaction<'info> {
        #[account(mut)]
        signer: Signer<'info>,
        #[account(mut)]
        multisig: Account<'info, Multisig>,
        #[account(
            init,
            seeds = [
                b"transaction",
                multisig.key().to_bytes().as_ref(),
                multisig.num_transactions.to_le_bytes().as_ref()
            ],
            bump = bump,
            payer = signer,
            space = transaction_space(instructions),
        )]
        transaction: Account<'info, Transaction>,
        system_program: Program<'info, System>,
    }

    pub fn create_transaction(
        ctx: Context<CreateTransaction>,
        instructions: Vec<TransactionInstruction>,
        bump: u8,
    ) -> ProgramResult {
        let multisig = &mut ctx.accounts.multisig;
        let tx = &mut ctx.accounts.transaction;
        let signer_key = ctx.accounts.signer.key;
        let owner_index = multisig
            .owners
            .iter()
            .position(|a| a == signer_key)
            .ok_or(ErrorCode::InvalidOwner)?;

        let mut signers = Vec::new();
        signers.resize(multisig.owners.len(), false);
        signers[owner_index] = true;

        tx.multisig = multisig.key();
        tx.bump = bump;
        tx.eta = Clock::get()?.unix_timestamp + multisig.delay;
        tx.owners_seq_no = multisig.owners_seq_no;
        tx.proposer = ctx.accounts.signer.key();
        tx.instructions = instructions.clone();
        tx.signers = signers;

        multisig.num_transactions = multisig
            .num_transactions
            .checked_add(1)
            .ok_or(ErrorCode::Overflow)?;
        Ok(())
    }

    #[derive(Accounts)]
    pub struct Approve<'info> {
        signer: Signer<'info>,
        multisig: Account<'info, Multisig>,
        #[account(mut, has_one = multisig)]
        transaction: Account<'info, Transaction>,
    }

    pub fn approve(ctx: Context<Approve>) -> ProgramResult {
        let owner_index = ctx
            .accounts
            .multisig
            .owners
            .iter()
            .position(|a| a == ctx.accounts.signer.key)
            .ok_or(ErrorCode::InvalidOwner)?;
        require!(
            ctx.accounts.multisig.owners_seq_no == ctx.accounts.transaction.owners_seq_no,
            OwnersChanged
        );
        ctx.accounts.transaction.signers[owner_index] = true;
        Ok(())
    }

    #[derive(Accounts)]
    pub struct ExecuteTransaction<'info> {
        #[account(
            signer,
            constraint = multisig.owners.contains(&signer.key()) @ ErrorCode::InvalidOwner
        )]
        signer: AccountInfo<'info>,
        multisig: Account<'info, Multisig>,
        #[account(mut, has_one = multisig)]
        transaction: Account<'info, Transaction>,
    }

    pub fn execute_transaction(ctx: Context<ExecuteTransaction>) -> ProgramResult {
        let tx = &mut ctx.accounts.transaction;

        let now = Clock::get()?.unix_timestamp;
        require!(now >= tx.eta, BeforeETA);
        require!(tx.executed_at == 0, AlreadyExecuted);
        require!(
            ctx.accounts.multisig.owners_seq_no == tx.owners_seq_no,
            OwnersChanged
        );

        // Do we have enough signers?
        let sig_count = tx.signers.iter().filter(|&signed| *signed).count();
        if sig_count < ctx.accounts.multisig.threshold as usize {
            return Err(ErrorCode::NotEnoughSigners.into());
        }

        tx.executed_at = now;
        tx.executor = ctx.accounts.signer.key();

        let seeds = &[
            b"multisig",
            ctx.accounts.multisig.base.as_ref(),
            &[ctx.accounts.multisig.bump],
        ];
        for ix in ctx.accounts.transaction.instructions.iter() {
            let six = solana_program::instruction::Instruction {
                program_id: ix.program_id,
                accounts: ix
                    .keys
                    .clone()
                    .into_iter()
                    .map(|a| solana_program::instruction::AccountMeta {
                        pubkey: a.pubkey,
                        is_signer: a.is_signer,
                        is_writable: a.is_writable,
                    })
                    .collect(),
                data: ix.data.clone(),
            };
            solana_program::program::invoke_signed(&six, ctx.remaining_accounts, &[seeds])?;
        }

        Ok(())
    }
}

pub fn require_unique_owners(owners: &[Pubkey]) -> Result<()> {
    let mut uniq_owners = owners.to_vec();
    uniq_owners.sort();
    uniq_owners.dedup();
    require!(owners.len() == uniq_owners.len(), UniqueOwners);
    Ok(())
}

pub fn transaction_space(instructions: Vec<TransactionInstruction>) -> usize {
    let mut space = 4 + std::mem::size_of::<Transaction>() + 4 + 15 + 4;
    for ix in instructions.iter() {
        space += std::mem::size_of::<Pubkey>()
            + (ix.keys.len() as usize) * std::mem::size_of::<TransactionInstructionMeta>()
            + (ix.data.len() as usize)
    }
    space
}
