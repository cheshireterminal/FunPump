use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("YourProgramID");

// Constants
pub const SECONDS_IN_DAY: i64 = 86400;
pub const MINIMUM_VESTING_PERIOD: i64 = SECONDS_IN_DAY * 7;    // 1 week
pub const MAXIMUM_VESTING_PERIOD: i64 = SECONDS_IN_DAY * 365 * 2;  // 2 years
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
        require!(amount > MINIMUM_AMOUNT, CustomError::InvalidAmount);
        require!(
            lock_duration >= MINIMUM_VESTING_PERIOD 
                && lock_duration <= MAXIMUM_VESTING_PERIOD,
            CustomError::InvalidTimeParameters
        );

        let current_time = Clock::get()?.unix_timestamp;
        let unlock_time = current_time
            .checked_add(lock_duration)
            .ok_or(CustomError::CalculationError)?;

        // Transfer from the user's token account into the vault's token account.
        token::transfer(ctx.accounts.into_transfer_to_vault_context(), amount)?;

        let vault = &mut ctx.accounts.vault;
        vault.locked_until = unlock_time;
        vault.locked_amount = amount;

        emit!(TokensLocked {
            vault_owner: vault.owner,
            locker_authority: ctx.accounts.authority.key(),
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

        let amount = vault.locked_amount;

        // Because the vault is a PDA, we must sign with the vault seeds to transfer tokens out.
        let vault_bump = vault.bump;
        let seeds = &[
            b"vault",
            vault.owner.as_ref(),
            &[vault_bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = token::Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.vault.to_account_info(), // The PDA
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::transfer(cpi_ctx, amount)?;

        vault.locked_amount = 0;

        emit!(TokensUnlocked {
            vault_owner: vault.owner,
            unlocker_authority: ctx.accounts.authority.key(),
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
        require!(amount > MINIMUM_AMOUNT, CustomError::InvalidVestingAmount);
        require!(end_time > start_time, CustomError::InvalidTimeParameters);

        let current_time = Clock::get()?.unix_timestamp;
        require!(start_time > current_time, CustomError::InvalidTimeParameters);

        let vesting_duration = end_time
            .checked_sub(start_time)
            .ok_or(CustomError::CalculationError)?;
        require!(
            vesting_duration >= MINIMUM_VESTING_PERIOD 
                && vesting_duration <= MAXIMUM_VESTING_PERIOD,
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

        // Sign with the vesting PDA seeds
        let seeds = &[
            b"vesting",
            vesting.token_mint.as_ref(),
            vesting.owner.as_ref(),
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

// -----------------
// Account Structs
// -----------------

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
    // The vault must have a matching owner Pubkey or else fail
    #[account(
        mut,
        has_one = owner @ CustomError::UnauthorizedAccess
    )]
    pub vault: Account<'info, Vault>,

    // The user (authority) is transferring tokens from their TokenAccount
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    // PDA vault’s token account
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,

    /// The actual signer doing the token transfer in (e.g. the user)
    pub authority: Signer<'info>,

    /// Must match the vault.owner
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct UnlockTokens<'info> {
    #[account(
        mut,
        has_one = owner @ CustomError::UnauthorizedAccess
    )]
    pub vault: Account<'info, Vault>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,   // Tokens go back here

    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,  // Vault's token account

    pub token_program: Program<'info, Token>,

    /// The user calling unlock
    pub authority: Signer<'info>,

    /// Must match the vault.owner
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct InitializeVesting<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = owner,
        space = 8 + Vesting::LEN,
        seeds = [b"vesting", token_mint.key().as_ref(), owner.key().as_ref()],
        bump
    )]
    pub vesting: Account<'info, Vesting>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct LockTokensForVesting<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner @ CustomError::UnauthorizedAccess
    )]
    pub vesting: Account<'info, Vesting>,

    #[account(mut)]
    pub owner_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub vesting_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct UnlockVestedTokens<'info> {
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner @ CustomError::UnauthorizedAccess
    )]
    pub vesting: Account<'info, Vesting>,

    #[account(mut)]
    pub vesting_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub owner_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

// -----------------
// State Accounts
// -----------------

#[account]
pub struct Vault {
    pub owner: Pubkey,       // Who owns/created this vault
    pub bump: u8,            // PDA bump
    pub locked_amount: u64,  // How many tokens currently locked
    pub locked_until: i64,   // Unix timestamp until which tokens are locked
}

impl Vault {
    pub const LEN: usize = 32 + 1 + 8 + 8; 
    // owner (32) + bump (1) + locked_amount (8) + locked_until (8)
}

#[account]
pub struct Vesting {
    pub owner: Pubkey,           // Who owns this vesting schedule
    pub token_mint: Pubkey,      // Which token is vested
    pub amount: u64,             // Total amount vested
    pub start_time: i64,         // When vesting *begins* 
    pub end_time: i64,           // When vesting ends (fully unlockable)
    pub target_market_cap: u64,  // Extra condition: must exceed this market cap
    pub is_locked: bool,         // If tokens are currently locked
    pub bump: u8,                // PDA bump
}

impl Vesting {
    pub const LEN: usize = 32 + 32 + 8 + 8 + 8 + 8 + 1 + 1;
    // owner (32) + token_mint (32) + amount (8) 
    // + start_time (8) + end_time (8) + target_market_cap (8) 
    // + is_locked (1) + bump (1)
}

// -----------------
// Events
// -----------------

#[event]
pub struct VaultInitialized {
    pub owner: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct TokensLocked {
    // Distinguish between who *owns* the vault (vault_owner)
    // vs. who *triggered* the lock (locker_authority).
    pub vault_owner: Pubkey,
    pub locker_authority: Pubkey,
    pub amount: u64,
    pub lock_until: i64,
}

#[event]
pub struct TokensUnlocked {
    pub vault_owner: Pubkey,
    pub unlocker_authority: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct VestingInitialized {
    pub owner: Pubkey,
    pub amount: u64,
    pub start_time: i64,
    pub end_time: i64,
}

#[event]
pub struct VestingTokensLocked {
    pub owner: Pubkey,
    pub amount: u64,
    pub vesting_account: Pubkey,
}

#[event]
pub struct VestingTokensUnlocked {
    pub owner: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

// ----------------------
// Error Codes
// ----------------------
#[error_code]
pub enum CustomError {
    #[msg("Unauthorized access")]
    UnauthorizedAccess,
    #[msg("Invalid time parameters")]
    InvalidTimeParameters,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Calculation error")]
    CalculationError,
    #[msg("Tokens are still locked")]
    TokensStillLocked,
    #[msg("Invalid vesting amount")]
    InvalidVestingAmount,
    #[msg("Tokens are already unlocked")]
    TokensAlreadyUnlocked,
    #[msg("Vesting period has not ended")]
    VestingPeriodNotEnded,
    #[msg("Market cap target not reached")]
    MarketCapNotReached,
    #[msg("Insufficient balance")]
    InsufficientBalance,
}

// ----------------------
// CPI Context Helpers
// ----------------------

impl<'info> LockTokens<'info> {
    // Transfers from the user’s token account -> vault token account
    fn into_transfer_to_vault_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.user_token_account.to_account_info(),
            to: self.vault_token_account.to_account_info(),
            authority: self.authority.to_account_info(), // user
        };
        CpiContext::new(self.token_program.to_account_info(), cpi_accounts)
    }
}

impl<'info> LockTokensForVesting<'info> {
    // Transfers from the user’s token account -> vesting PDA token account
    fn into_transfer_to_vesting_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.owner_token_account.to_account_info(),
            to: self.vesting_token_account.to_account_info(),
            authority: self.owner.to_account_info(), // user
        };
        CpiContext::new(self.token_program.to_account_info(), cpi_accounts)
    }
}
