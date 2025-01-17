use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use std::convert::TryInto;

declare_id!("YourProgramID");

// -----------------------------------------------------------------
// Constants
// -----------------------------------------------------------------
pub const SECONDS_IN_DAY: i64 = 86400;
pub const MINIMUM_VESTING_PERIOD: i64 = SECONDS_IN_DAY * 7;    // 1 week
pub const MAXIMUM_VESTING_PERIOD: i64 = SECONDS_IN_DAY * 365 * 2;  // 2 years
pub const MINIMUM_AMOUNT: u64 = 1;
pub const BASIS_POINTS: u16 = 10000; // For percentage calculations

// -----------------------------------------------------------------
// Enums
// -----------------------------------------------------------------
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq)]
pub enum VestingScheduleType {
    Linear,
    Staggered,
    Cliff,
    CustomMilestone,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq)]
pub enum StreamType {
    Linear,
    Exponential,
    Custom,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq)]
pub enum CurveType {
    Linear,
    Exponential,
    Sigmoid,
    Custom,
}

// -----------------------------------------------------------------
// Data Structures (Accounts)
// -----------------------------------------------------------------

#[account]
pub struct TokenLaunch {
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub total_supply: u64,
    pub curve: Curve,
    // Additional vesting/stream config if desired
    pub is_initialized: bool,
    pub bump: u8,
}

// Example curve struct for bonding curve logic
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct Curve {
    pub curve_type: CurveType,
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub initial_virtual_token_reserves: u64,
    pub custom_params: [u64; 3],
}

// -----------------------------------------------------------------
// Error Codes
// -----------------------------------------------------------------
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
    #[msg("Tokens still locked")]
    TokensStillLocked,
    #[msg("Invalid vesting amount")]
    InvalidVestingAmount,
    #[msg("Tokens already unlocked")]
    TokensAlreadyUnlocked,
    #[msg("Vesting period not ended")]
    VestingPeriodNotEnded,
    #[msg("Market cap not reached")]
    MarketCapNotReached,
    #[msg("Insufficient balance")]
    InsufficientBalance,
    #[msg("Invalid curve parameters")]
    InvalidCurveParameters,
    #[msg("Invalid milestone")]
    InvalidMilestone,
    #[msg("Stream not initialized")]
    StreamNotInitialized,
    #[msg("Invalid stream rate")]
    InvalidStreamRate,
}

// -----------------------------------------------------------------
// Curve Implementation
// -----------------------------------------------------------------
impl Curve {
    pub fn initialize(
        &mut self,
        curve_type: CurveType,
        virtual_sol: u64,
        virtual_token: u64,
        custom_params: [u64; 3],
    ) -> Result<()> {
        require!(virtual_sol > 0 && virtual_token > 0, CustomError::InvalidCurveParameters);

        self.curve_type = curve_type;
        self.virtual_sol_reserves = virtual_sol;
        self.virtual_token_reserves = virtual_token;
        self.real_sol_reserves = 0;
        self.real_token_reserves = virtual_token;
        self.initial_virtual_token_reserves = virtual_token;
        self.custom_params = custom_params;

        Ok(())
    }

    pub fn calculate_buy_price(&self, amount: u64) -> Result<u64> {
        require!(amount > 0, CustomError::InvalidAmount);
        
        match self.curve_type {
            CurveType::Linear => self.calculate_linear_buy_price(amount),
            CurveType::Exponential => self.calculate_exponential_buy_price(amount),
            CurveType::Sigmoid => self.calculate_sigmoid_buy_price(amount),
            CurveType::Custom => self.calculate_custom_buy_price(amount),
        }
    }

    pub fn calculate_sell_price(&self, amount: u64) -> Result<u64> {
        require!(amount > 0, CustomError::InvalidAmount);
        
        match self.curve_type {
            CurveType::Linear => self.calculate_linear_sell_price(amount),
            CurveType::Exponential => self.calculate_exponential_sell_price(amount),
            CurveType::Sigmoid => self.calculate_sigmoid_sell_price(amount),
            CurveType::Custom => self.calculate_custom_sell_price(amount),
        }
    }

    // -----------------------------
    // Linear Bonding Curve
    // -----------------------------
    fn calculate_linear_buy_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let slope = self.custom_params[0] as u128;

        // base price
        let price = (virtual_sol.saturating_mul(amount)) / virtual_token;
        // linear factor
        let linear_factor = (amount.saturating_mul(slope)) / (BASIS_POINTS as u128);

        let total_price = price
            .checked_add(linear_factor)
            .ok_or(CustomError::CalculationError)?;

        Ok(total_price as u64)
    }

    fn calculate_linear_sell_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let slope = self.custom_params[0] as u128;

        let base_price = (virtual_sol.saturating_mul(amount)) / virtual_token;
        let linear_factor = (amount.saturating_mul(slope)) / (BASIS_POINTS as u128);
        
        let total_price = base_price
            .checked_sub(linear_factor)
            .ok_or(CustomError::CalculationError)?;

        // Cap to not exceed real_sol_reserves
        Ok((total_price as u64).min(self.real_sol_reserves))
    }

    // -----------------------------
    // Exponential Bonding Curve
    // -----------------------------
    fn calculate_exponential_buy_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let exponent = self.custom_params[1] as u128;

        let base_price = (virtual_sol.saturating_mul(amount)) / virtual_token;
        // This is a simplistic approach; watch for overflow
        let exp_factor = ((amount.saturating_mul(exponent)) / (BASIS_POINTS as u128)).pow(2);

        let total_price = base_price
            .checked_add(exp_factor)
            .ok_or(CustomError::CalculationError)?;

        Ok(total_price as u64)
    }

    fn calculate_exponential_sell_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let exponent = self.custom_params[1] as u128;

        let base_price = (virtual_sol.saturating_mul(amount)) / virtual_token;
        let exp_factor = ((amount.saturating_mul(exponent)) / (BASIS_POINTS as u128)).pow(2);

        let total_price = base_price
            .checked_sub(exp_factor)
            .ok_or(CustomError::CalculationError)?;

        Ok((total_price as u64).min(self.real_sol_reserves))
    }

    // -----------------------------
    // Sigmoid Bonding Curve
    // -----------------------------
    fn calculate_sigmoid_buy_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let midpoint = self.custom_params[2] as u128;

        let x = (amount.saturating_mul(BASIS_POINTS as u128)) / virtual_token;
        let sigmoid = self.sigmoid(x, midpoint)?;

        let price = (virtual_sol.saturating_mul(amount).saturating_mul(sigmoid))
            / (virtual_token.saturating_mul(BASIS_POINTS as u128));

        Ok(price as u64)
    }

    fn calculate_sigmoid_sell_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let midpoint = self.custom_params[2] as u128;

        let x = (amount.saturating_mul(BASIS_POINTS as u128)) / virtual_token;
        let sigmoid = self.sigmoid(x, midpoint)?;

        let price = (virtual_sol.saturating_mul(amount).saturating_mul(sigmoid))
            / (virtual_token.saturating_mul(BASIS_POINTS as u128));

        Ok((price as u64).min(self.real_sol_reserves))
    }

    // -----------------------------
    // Custom Bonding Curve
    // -----------------------------
    fn calculate_custom_buy_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;

        let slope = self.custom_params[0] as u128;
        let exponent = self.custom_params[1] as u128;
        let midpoint = self.custom_params[2] as u128;

        let base_price = (virtual_sol.saturating_mul(amount)) / virtual_token;
        let custom_factor = (amount.saturating_mul(slope).saturating_mul(exponent))
            / (midpoint.saturating_mul(BASIS_POINTS as u128));

        let total_price = base_price
            .checked_add(custom_factor)
            .ok_or(CustomError::CalculationError)?;

        Ok(total_price as u64)
    }

    fn calculate_custom_sell_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;

        let slope = self.custom_params[0] as u128;
        let exponent = self.custom_params[1] as u128;
        let midpoint = self.custom_params[2] as u128;

        let base_price = (virtual_sol.saturating_mul(amount)) / virtual_token;
        let custom_factor = (amount.saturating_mul(slope).saturating_mul(exponent))
            / (midpoint.saturating_mul(BASIS_POINTS as u128));

        let total_price = base_price
            .checked_sub(custom_factor)
            .ok_or(CustomError::CalculationError)?;

        Ok((total_price as u64).min(self.real_sol_reserves))
    }

    // -----------------------------
    // Helper: Sigmoid
    // -----------------------------
    fn sigmoid(&self, x: u128, midpoint: u128) -> Result<u128> {
        // Example simplified sigmoid: x / (x + midpoint)
        let numerator = x.checked_mul(BASIS_POINTS as u128)
            .ok_or(CustomError::CalculationError)?;

        let denominator = x.checked_add(midpoint)
            .ok_or(CustomError::CalculationError)?;

        Ok(numerator / denominator)
    }

    // -----------------------------
    // Update Reserves
    // -----------------------------
    pub fn update_reserves(&mut self, sol_delta: i64, token_delta: i64) -> Result<()> {
        if sol_delta > 0 {
            self.real_sol_reserves = self.real_sol_reserves
                .checked_add(sol_delta as u64)
                .ok_or(CustomError::CalculationError)?;
        } else if sol_delta < 0 {
            self.real_sol_reserves = self.real_sol_reserves
                .checked_sub(sol_delta.abs() as u64)
                .ok_or(CustomError::CalculationError)?;
        }

        if token_delta > 0 {
            self.real_token_reserves = self.real_token_reserves
                .checked_add(token_delta as u64)
                .ok_or(CustomError::CalculationError)?;
        } else if token_delta < 0 {
            self.real_token_reserves = self.real_token_reserves
                .checked_sub(token_delta.abs() as u64)
                .ok_or(CustomError::CalculationError)?;
        }

        Ok(())
    }
}

// -----------------------------------------------------------------
// Program Module
// -----------------------------------------------------------------
#[program]
pub mod token_launch_program {
    use super::*;

    // -------------------------------------
    // 1) Initialize the Launch
    // -------------------------------------
    pub fn initialize_launch(
        ctx: Context<InitializeLaunch>,
        total_supply: u64,
        virtual_sol: u64,
        virtual_token: u64,
        curve_type: CurveType,
        custom_params: [u64; 3],
    ) -> Result<()> {
        let launch = &mut ctx.accounts.token_launch;
        launch.creator = ctx.accounts.creator.key();
        launch.mint = ctx.accounts.mint.key();
        launch.total_supply = total_supply;
        launch.is_initialized = true;
        launch.bump = *ctx.bumps.get("token_launch").unwrap();

        // Initialize the curve
        launch.curve.initialize(
            curve_type,
            virtual_sol,
            virtual_token,
            custom_params,
        )?;

        emit!(LaunchInitialized {
            creator: launch.creator,
            mint: launch.mint,
            total_supply,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    // -------------------------------------
    // 2) Buy Tokens
    // -------------------------------------
    pub fn buy_tokens(
        ctx: Context<TradeTokens>,
        amount: u64,
    ) -> Result<()> {
        let launch = &mut ctx.accounts.token_launch;
        let sol_amount = launch.curve.calculate_buy_price(amount)?;

        // Check trader's lamports
        require!(
            ctx.accounts.trader.lamports() >= sol_amount,
            CustomError::InsufficientBalance
        );

        // Transfer SOL from trader to sol_vault
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.trader.to_account_info(),
                to: ctx.accounts.sol_vault.to_account_info(),
            },
        );
        system_program::transfer(cpi_context, sol_amount)?;

        // Transfer tokens (SPL) from token_vault to trader
        let seeds = &[
            b"token_launch",
            launch.mint.as_ref(),
            &[launch.bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = token::Transfer {
            from: ctx.accounts.token_vault.to_account_info(),
            to: ctx.accounts.trader_token_account.to_account_info(),
            authority: launch.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

        token::transfer(cpi_ctx, amount)?;

        // Update curve reserves
        launch.curve.update_reserves(sol_amount as i64, -(amount as i64))?;

        emit!(TokensPurchased {
            trader: ctx.accounts.trader.key(),
            token_amount: amount,
            sol_amount,
        });

        Ok(())
    }

    // -------------------------------------
    // 3) Sell Tokens
    // -------------------------------------
    pub fn sell_tokens(
        ctx: Context<TradeTokens>,
        amount: u64,
    ) -> Result<()> {
        let launch = &mut ctx.accounts.token_launch;
        let sol_amount = launch.curve.calculate_sell_price(amount)?;

        // Check if sol_vault has enough lamports
        require!(
            ctx.accounts.sol_vault.lamports() >= sol_amount,
            CustomError::InsufficientBalance
        );

        // Transfer tokens (SPL) from trader to token_vault
        let cpi_accounts = token::Transfer {
            from: ctx
                .accounts
                .trader_token_account
                .to_account_info(),
            to: ctx.accounts.token_vault.to_account_info(),
            authority: ctx.accounts.trader.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        token::transfer(cpi_ctx, amount)?;

        // Transfer SOL from sol_vault to trader, using PDA seeds
        let seeds = &[
            b"sol_vault",
            ctx.accounts.token_launch.key().as_ref(),
            // NOTE: If you want a separate bump for the sol_vault,
            // store it in the TokenLaunch or generate it. For now,
            // we reuse `launch.bump`, but be consistent with how
            // you derived `sol_vault`.
            &[launch.bump],
        ];
        let signer = &[&seeds[..]];

        // Manually adjust lamports
        **ctx.accounts.sol_vault.try_borrow_mut_lamports()? = ctx
            .accounts
            .sol_vault
            .lamports()
            .checked_sub(sol_amount)
            .ok_or(CustomError::CalculationError)?;

        **ctx.accounts.trader.try_borrow_mut_lamports()? = ctx
            .accounts
            .trader
            .lamports()
            .checked_add(sol_amount)
            .ok_or(CustomError::CalculationError)?;

        // Update curve reserves
        launch.curve.update_reserves(-(sol_amount as i64), amount as i64)?;

        emit!(TokensSold {
            trader: ctx.accounts.trader.key(),
            token_amount: amount,
            sol_amount,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------
// Context Structures
// -----------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeLaunch<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    
    #[account(
        init,
        payer = creator,
        space = 8 + std::mem::size_of::<TokenLaunch>(),
        seeds = [b"token_launch", mint.key().as_ref()],
        bump
    )]
    pub token_launch: Account<'info, TokenLaunch>,
    
    pub mint: Account<'info, Mint>,
    
    #[account(
        init,
        payer = creator,
        associated_token::mint = mint,
        associated_token::authority = token_launch
    )]
    pub token_vault: Account<'info, TokenAccount>,
    
    /// PDA system account to hold raw SOL.
    /// We use a separate seed: [b"sol_vault", token_launch.key().as_ref()].
    #[account(
        init,
        payer = creator,
        space = 8, // minimal space, or more if needed for future data
        seeds = [b"sol_vault", token_launch.key().as_ref()],
        bump
    )]
    pub sol_vault: SystemAccount<'info>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct TradeTokens<'info> {
    #[account(mut)]
    pub token_launch: Account<'info, TokenLaunch>,
    
    #[account(mut)]
    pub trader: Signer<'info>,
    
    #[account(mut)]
    pub token_vault: Account<'info, TokenAccount>,
    
    /// The raw‑SOL vault (PDA). 
    /// The seeds must match the above “sol_vault” derivation.
    #[account(
        mut,
        seeds = [b"sol_vault", token_launch.key().as_ref()],
        bump
    )]
    pub sol_vault: SystemAccount<'info>,
    
    #[account(mut)]
    pub trader_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------
// Updated Events
// -----------------------------------------------------------------
#[event]
pub struct LaunchInitialized {
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub total_supply: u64,
    pub timestamp: i64,
}

#[event]
pub struct TokensPurchased {
    pub trader: Pubkey,
    pub token_amount: u64,
    pub sol_amount: u64,
}

#[event]
pub struct TokensSold {
    pub trader: Pubkey,
    pub token_amount: u64,
    pub sol_amount: u64,
}
