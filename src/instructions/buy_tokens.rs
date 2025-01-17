use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use crate::state::Curve;
use crate::utils::curve_calculations::calculate_tokens_out;

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,
    #[account(mut)]
    pub curve: Account<'info, Curve>,
    #[account(mut)]
    pub sol_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub token_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub buyer_token_account: Account<'info, TokenAccount>,
    pub mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<BuyTokens>, amount: u64) -> Result<()> {
    let curve = &mut ctx.accounts.curve;
    let tokens_out = calculate_tokens_out(curve, amount)?;
    
    // Transfer SOL from buyer to pool
    let cpi_context = CpiContext::new(
        ctx.accounts.system_program.to_account_info(),
        anchor_lang::system_program::Transfer {
            from: ctx.accounts.buyer.to_account_info(),
            to: ctx.accounts.sol_vault.to_account_info(),
        },
    );
    anchor_lang::system_program::transfer(cpi_context, amount)?;

    // Transfer tokens from pool to buyer
    let seeds = &[
        b"curve".as_ref(),
        &ctx.accounts.mint.key().to_bytes(),
        &[curve.bump],
    ];
    let signer = &[&seeds[..]];
    let cpi_accounts = token::Transfer {
        from: ctx.accounts.token_vault.to_account_info(),
        to: ctx.accounts.buyer_token_account.to_account_info(),
        authority: curve.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
    token::transfer(cpi_ctx, tokens_out)?;

    // Update curve state
    curve.reserve_token -= tokens_out;
    curve.reserve_sol += amount;

    Ok(())
}