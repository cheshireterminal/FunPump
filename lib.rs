use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use std::convert::TryInto;

declare_id!("YourProgramID");

// Constants for better maintainability
pub const SECONDS_IN_DAY: i64 = 86400;
pub const MINIMUM_VESTING_PERIOD: i64 = SECONDS_IN_DAY * 7; // 1 week
pub const MAXIMUM_VESTING_PERIOD: i64 = SECONDS_IN_DAY * 365 * 2; // 2 years
pub const MINIMUM_AMOUNT: u64 = 1;

#[program]
pub mod complete_solana_project {
    use super::*;

    pub fn initialize_vault(ctx: Context<InitializeVault>, bump: u8) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        vault.owner = *ctx.accounts.payer.key;
        vault.bump = bump;
        vault.locked_amount = 0;
        vault.locked_until = 0;

        emit!(VaultInitialized {
            owner: vault.owner,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }

    pub fn lock_tokens(ctx: Context<LockTokens>, amount: u64, lock_duration: i64) -> Result<()> {
        // Validate inputs
        require!(amount > MINIMUM_AMOUNT, CustomError::InvalidAmount);
        require!(lock_duration >= MINIMUM_VESTING_PERIOD, CustomError::InvalidTimeParameters);
        require!(lock_duration <= MAXIMUM_VESTING_PERIOD, CustomError::InvalidTimeParameters);

        let current_time = Clock::get()?.unix_timestamp;
        let unlock_time = current_time.checked_add(lock_duration)
            .ok_or(CustomError::CalculationError)?;

        token::transfer(ctx.accounts.into_transfer_to_vault_context(), amount)?;

        let vault = &mut ctx.accounts.vault;
        vault.locked_until = unlock_time;
        vault.locked_amount = amount;

        emit!(TokensLocked {
            owner: ctx.accounts.authority.key(),
            amount,
            lock_until: unlock_time,
        });
        Ok(())
    }

    pub fn unlock_tokens(ctx: Context<UnlockTokens>) -> Result<()> {
        let current_time = Clock::get()?.unix_timestamp;
        let vault = &mut ctx.accounts.vault;

        require!(current_time >= vault.locked_until, CustomError::TokensStillLocked);
        require!(vault.locked_amount > 0, CustomError::InsufficientBalance);

        // Save amount before transfer for event emission
        let amount = vault.locked_amount;

        token::transfer(ctx.accounts.into_transfer_from_vault_context(), vault.locked_amount)?;

        vault.locked_amount = 0;

        emit!(TokensUnlocked {
            owner: ctx.accounts.authority.key(),
            amount,
            timestamp: current_time,
        });
        Ok(())
    }

    pub fn initialize_vesting(
        ctx: Context<InitializeVesting>,
        amount: u64,
        start_time: i64,
        end_time: i64,
        target_market_cap: u64,
    ) -> Result<()> {
        // Validate inputs
        require!(amount > MINIMUM_AMOUNT, CustomError::InvalidVestingAmount);
        require!(end_time > start_time, CustomError::InvalidTimeParameters);
        
        let current_time = Clock::get()?.unix_timestamp;
        require!(start_time > current_time, CustomError::InvalidTimeParameters);
        
        let vesting_duration = end_time.checked_sub(start_time)
            .ok_or(CustomError::CalculationError)?;
        require!(
            vesting_duration >= MINIMUM_VESTING_PERIOD && 
            vesting_duration <= MAXIMUM_VESTING_PERIOD,
            CustomError::InvalidTimeParameters
        );

        let vesting = &mut ctx.accounts.vesting;
        vesting.owner = ctx.accounts.owner.key();
        vesting.token_mint = ctx.accounts.token_mint.key();
        vesting.amount = amount;
        vesting.start_time = start_time;
        vesting.end_time = end_time;
        vesting.target_market_cap = target_market_cap;
        vesting.is_locked = true;
        vesting.bump = *ctx.bumps.get("vesting").unwrap();

        emit!(VestingInitialized {
            owner: vesting.owner,
            amount,
            start_time,
            end_time,
        });
        Ok(())
    }

    pub fn lock_tokens_for_vesting(ctx: Context<LockTokensForVesting>, amount: u64) -> Result<()> {
        let vesting = &mut ctx.accounts.vesting;
        require!(amount == vesting.amount, CustomError::InvalidVestingAmount);
        require!(vesting.is_locked, CustomError::TokensAlreadyUnlocked);

        token::transfer(ctx.accounts.into_transfer_to_vesting_context(), amount)?;

        emit!(VestingTokensLocked {
            owner: ctx.accounts.owner.key(),
            amount,
            vesting_account: ctx.accounts.vesting.key(),
        });
        Ok(())
    }

    pub fn unlock_vested_tokens(ctx: Context<UnlockVestedTokens>, current_market_cap: u64) -> Result<()> {
        let vesting = &mut ctx.accounts.vesting;
        let current_time = Clock::get()?.unix_timestamp;

        require!(vesting.is_locked, CustomError::TokensAlreadyUnlocked);
        require!(current_time >= vesting.end_time, CustomError::VestingPeriodNotEnded);
        require!(
            current_market_cap >= vesting.target_market_cap, 
            CustomError::MarketCapNotReached
        );

        let seeds = &[
            b"vesting".as_ref(),
            &vesting.token_mint.to_bytes(),
            &vesting.owner.to_bytes(),
            &[vesting.bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = token::Transfer {
            from: ctx.accounts.vesting_token_account.to_account_info(),
            to: ctx.accounts.owner_token_account.to_account_info(),
            authority: vesting.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        
        token::transfer(cpi_ctx, vesting.amount)?;

        vesting.is_locked = false;

        emit!(VestingTokensUnlocked {
            owner: ctx.accounts.owner.key(),
            amount: vesting.amount,
            timestamp: current_time,
        });
        Ok(())
    }
}

// Account Structures
#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + Vault::LEN,
        seeds = [b"vault", payer.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, Vault>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct LockTokens<'info> {
    #[account(
        mut,
        has_one = owner @ CustomError::UnauthorizedAccess
    )]
    pub vault: Account<'info, Vault>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub authority: Signer<'info>,
}

// ... (continuing in next message due to length)
