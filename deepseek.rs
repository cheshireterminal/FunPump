use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use std::convert::TryInto;

declare_id!("GVapdHoG4xjJZpvGPd8EUBaUJKR5Txpf6VHnVwBVCY69");

#[program]
pub mod curve_launchpad {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::initialize(ctx)
    }

    pub fn create(ctx: Context<Create>, name: String, symbol: String, uri: String) -> Result<()> {
        instructions::create::create(ctx, name, symbol, uri)
    }

    pub fn buy(ctx: Context<Buy>, token_amount: u64, max_sol_cost: u64) -> Result<()> {
        instructions::buy::buy(ctx, token_amount, max_sol_cost)
    }

    pub fn sell(ctx: Context<Sell>, token_amount: u64, min_sol_output: u64) -> Result<()> {
        instructions::sell::sell(ctx, token_amount, min_sol_output)
    }

    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        instructions::withdraw::withdraw(ctx)
    }

    pub fn set_params(
        ctx: Context<SetParams>,
        fee_recipient: Pubkey,
        withdraw_authority: Pubkey,
        initial_virtual_token_reserves: u64,
        initial_virtual_sol_reserves: u64,
        initial_real_token_reserves: u64,
        initial_token_supply: u64,
        fee_basis_points: u64,
    ) -> Result<()> {
        instructions::set_params::set_params(
            ctx,
            fee_recipient,
            withdraw_authority,
            initial_virtual_token_reserves,
            initial_virtual_sol_reserves,
            initial_real_token_reserves,
            initial_token_supply,
            fee_basis_points,
        )
    }
}

pub mod instructions {
    use super::*;

    pub mod initialize {
        use super::*;

        #[derive(Accounts)]
        pub struct Initialize<'info> {
            #[account(init, payer = user, space = 8 + std::mem::size_of::<state::Launchpad>())]
            pub launchpad: Account<'info, state::Launchpad>,
            #[account(mut)]
            pub user: Signer<'info>,
            pub system_program: Program<'info, System>,
        }

        pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
            let launchpad = &mut ctx.accounts.launchpad;
            launchpad.authority = *ctx.accounts.user.key;
            Ok(())
        }
    }

    pub mod create {
        use super::*;

        #[derive(Accounts)]
        pub struct Create<'info> {
            #[account(mut)]
            pub launchpad: Account<'info, state::Launchpad>,
            #[account(mut)]
            pub user: Signer<'info>,
            #[account(init, payer = user, space = 8 + std::mem::size_of::<state::TokenMetadata>())]
            pub token_metadata: Account<'info, state::TokenMetadata>,
            pub system_program: Program<'info, System>,
        }

        pub fn create(ctx: Context<Create>, name: String, symbol: String, uri: String) -> Result<()> {
            let token_metadata = &mut ctx.accounts.token_metadata;
            token_metadata.name = name;
            token_metadata.symbol = symbol;
            token_metadata.uri = uri;
            Ok(())
        }
    }

    pub mod buy {
        use super::*;

        #[derive(Accounts)]
        pub struct Buy<'info> {
            #[account(mut)]
            pub launchpad: Account<'info, state::Launchpad>,
            #[account(mut)]
            pub user: Signer<'info>,
            #[account(mut)]
            pub user_token_account: Account<'info, TokenAccount>,
            #[account(mut)]
            pub token_vault: Account<'info, TokenAccount>,
            #[account(mut)]
            pub sol_vault: SystemAccount<'info>,
            pub token_program: Program<'info, Token>,
            pub system_program: Program<'info, System>,
        }

        pub fn buy(ctx: Context<Buy>, token_amount: u64, max_sol_cost: u64) -> Result<()> {
            let launchpad = &mut ctx.accounts.launchpad;
            let sol_amount = launchpad.curve.calculate_buy_price(token_amount)?;

            require!(sol_amount <= max_sol_cost, CustomError::InsufficientBalance);

            let cpi_context = CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.user.to_account_info(),
                    to: ctx.accounts.sol_vault.to_account_info(),
                },
            );
            system_program::transfer(cpi_context, sol_amount)?;

            let seeds = &[b"token_launch", launchpad.mint.as_ref(), &[launchpad.bump]];
            let signer = &[&seeds[..]];

            let cpi_accounts = token::Transfer {
                from: ctx.accounts.token_vault.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: launchpad.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

            token::transfer(cpi_ctx, token_amount)?;

            launchpad.curve.update_reserves(sol_amount as i64, -(token_amount as i64))?;

            emit!(TokensPurchased {
                trader: ctx.accounts.user.key(),
                token_amount,
                sol_amount,
            });

            Ok(())
        }
    }

    pub mod sell {
        use super::*;

        #[derive(Accounts)]
        pub struct Sell<'info> {
            #[account(mut)]
            pub launchpad: Account<'info, state::Launchpad>,
            #[account(mut)]
            pub user: Signer<'info>,
            #[account(mut)]
            pub user_token_account: Account<'info, TokenAccount>,
            #[account(mut)]
            pub token_vault: Account<'info, TokenAccount>,
            #[account(mut)]
            pub sol_vault: SystemAccount<'info>,
            pub token_program: Program<'info, Token>,
            pub system_program: Program<'info, System>,
        }

        pub fn sell(ctx: Context<Sell>, token_amount: u64, min_sol_output: u64) -> Result<()> {
            let launchpad = &mut ctx.accounts.launchpad;
            let sol_amount = launchpad.curve.calculate_sell_price(token_amount)?;

            require!(sol_amount >= min_sol_output, CustomError::InsufficientBalance);

            let cpi_accounts = token::Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.token_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

            token::transfer(cpi_ctx, token_amount)?;

            let sol_vault_lamports = ctx.accounts.sol_vault.lamports();
            let user_lamports = ctx.accounts.user.lamports();

            **ctx.accounts.sol_vault.try_borrow_mut_lamports()? = sol_vault_lamports
                .checked_sub(sol_amount)
                .ok_or(CustomError::CalculationError)?;

            **ctx.accounts.user.try_borrow_mut_lamports()? = user_lamports
                .checked_add(sol_amount)
                .ok_or(CustomError::CalculationError)?;

            launchpad.curve.update_reserves(-(sol_amount as i64), token_amount as i64)?;

            emit!(TokensSold {
                trader: ctx.accounts.user.key(),
                token_amount,
                sol_amount,
            });

            Ok(())
        }
    }

    pub mod withdraw {
        use super::*;

        #[derive(Accounts)]
        pub struct Withdraw<'info> {
            #[account(mut)]
            pub launchpad: Account<'info, state::Launchpad>,
            #[account(mut)]
            pub authority: Signer<'info>,
            #[account(mut)]
            pub fee_recipient: AccountInfo<'info>,
            #[account(mut)]
            pub sol_vault: SystemAccount<'info>,
            pub system_program: Program<'info, System>,
        }

        pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
            let launchpad = &mut ctx.accounts.launchpad;
            let sol_amount = launchpad.curve.real_sol_reserves;

            require!(
                ctx.accounts.authority.key() == &launchpad.authority,
                CustomError::UnauthorizedAccess
            );

            let sol_vault_lamports = ctx.accounts.sol_vault.lamports();
            let fee_recipient_lamports = ctx.accounts.fee_recipient.lamports();

            **ctx.accounts.sol_vault.try_borrow_mut_lamports()? = sol_vault_lamports
                .checked_sub(sol_amount)
                .ok_or(CustomError::CalculationError)?;

            **ctx.accounts.fee_recipient.try_borrow_mut_lamports()? = fee_recipient_lamports
                .checked_add(sol_amount)
                .ok_or(CustomError::CalculationError)?;

            launchpad.curve.real_sol_reserves = 0;

            Ok(())
        }
    }

    pub mod set_params {
        use super::*;

        #[derive(Accounts)]
        pub struct SetParams<'info> {
            #[account(mut)]
            pub launchpad: Account<'info, state::Launchpad>,
            #[account(mut)]
            pub authority: Signer<'info>,
        }

        pub fn set_params(
            ctx: Context<SetParams>,
            fee_recipient: Pubkey,
            withdraw_authority: Pubkey,
            initial_virtual_token_reserves: u64,
            initial_virtual_sol_reserves: u64,
            initial_real_token_reserves: u64,
            initial_token_supply: u64,
            fee_basis_points: u64,
        ) -> Result<()> {
            let launchpad = &mut ctx.accounts.launchpad;

            require!(
                ctx.accounts.authority.key() == &launchpad.authority,
                CustomError::UnauthorizedAccess
            );

            launchpad.curve = state::Curve {
                curve_type: state::CurveType::Linear,
                virtual_sol_reserves: initial_virtual_sol_reserves,
                virtual_token_reserves: initial_virtual_token_reserves,
                real_sol_reserves: 0,
                real_token_reserves: initial_real_token_reserves,
                initial_virtual_token_reserves,
                custom_params: [0; 3],
            };

            launchpad.fee_recipient = fee_recipient;
            launchpad.withdraw_authority = withdraw_authority;
            launchpad.fee_basis_points = fee_basis_points;

            Ok(())
        }
    }
}

pub mod state {
    use super::*;

    #[account]
    pub struct Launchpad {
        pub authority: Pubkey,
        pub mint: Pubkey,
        pub curve: Curve,
        pub fee_recipient: Pubkey,
        pub withdraw_authority: Pubkey,
        pub fee_basis_points: u64,
        pub bump: u8,
    }

    #[account]
    pub struct TokenMetadata {
        pub name: String,
        pub symbol: String,
        pub uri: String,
    }

    #[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq)]
    pub enum CurveType {
        Linear,
        Exponential,
        Sigmoid,
        Custom,
    }

    #[account]
    #[derive(Default)]
    pub struct Curve {
        pub curve_type: CurveType,
        pub virtual_sol_reserves: u64,
        pub virtual_token_reserves: u64,
        pub real_sol_reserves: u64,
        pub real_token_reserves: u64,
        pub initial_virtual_token_reserves: u64,
        pub custom_params: [u64; 3],
    }

    impl Curve {
        pub fn calculate_buy_price(&self, amount: u64) -> Result<u64> {
            match self.curve_type {
                CurveType::Linear => self.calculate_linear_buy_price(amount),
                _ => Err(CustomError::InvalidCurveParameters.into()),
            }
        }

        pub fn calculate_sell_price(&self, amount: u64) -> Result<u64> {
            match self.curve_type {
                CurveType::Linear => self.calculate_linear_sell_price(amount),
                _ => Err(CustomError::InvalidCurveParameters.into()),
            }
        }

        fn calculate_linear_buy_price(&self, amount: u64) -> Result<u64> {
            let amount = amount as u128;
            let virtual_sol = self.virtual_sol_reserves as u128;
            let virtual_token = self.virtual_token_reserves as u128;

            let price = (virtual_sol * amount) / virtual_token;
            Ok(price as u64)
        }

        fn calculate_linear_sell_price(&self, amount: u64) -> Result<u64> {
            let amount = amount as u128;
            let virtual_sol = self.virtual_sol_reserves as u128;
            let virtual_token = self.virtual_token_reserves as u128;

            let price = (virtual_sol * amount) / virtual_token;
            Ok(price as u64)
        }

        pub fn update_reserves(&mut self, sol_delta: i64, token_delta: i64) -> Result<()> {
            if sol_delta > 0 {
                self.real_sol_reserves = self
                    .real_sol_reserves
                    .checked_add(sol_delta as u64)
                    .ok_or(CustomError::CalculationError)?;
            } else if sol_delta < 0 {
                self.real_sol_reserves = self
                    .real_sol_reserves
                    .checked_sub(sol_delta.abs() as u64)
                    .ok_or(CustomError::CalculationError)?;
            }

            if token_delta > 0 {
                self.real_token_reserves = self
                    .real_token_reserves
                    .checked_add(token_delta as u64)
                    .ok_or(CustomError::CalculationError)?;
            } else if token_delta < 0 {
                self.real_token_reserves = self
                    .real_token_reserves
                    .checked_sub(token_delta.abs() as u64)
                    .ok_or(CustomError::CalculationError)?;
            }

            Ok(())
        }
    }
}

#[error_code]
pub enum CustomError {
    #[msg("Unauthorized access")]
    UnauthorizedAccess,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Calculation error")]
    CalculationError,
    #[msg("Insufficient balance")]
    InsufficientBalance,
    #[msg("Invalid curve parameters")]
    InvalidCurveParameters,
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
