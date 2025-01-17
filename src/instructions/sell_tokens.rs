use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use crate::state::Curve;
use crate::utils::curve_calculations::calculate_sol_out;

#[derive(Accounts)]
pub struct SellTokens<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,
    #[account(mut)]
    pub curve: Account<'info, Curve>,
    #[account(mut)]
    pub sol_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub token_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub seller_token_account: Account<'info, TokenAccount>,
    pub mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<SellTokens>, amount: u64) -> Result<()> {
    let curve = &mut ctx.accounts.curve;
    let sol_out = calculate_sol_out(curve, amount)?;

    // Transfer tokens from seller to pool
    let cpi_accounts = token::Transfer {
        from: ctx.accounts.seller_token_account.to_account_info(),
        to: ctx.accounts.token_vault.to_account_info(),
        authority: ctx.accounts.seller.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    token::transfer(cpi_ctx, amount)?;

    // Transfer SOL from pool to seller
    let seeds = &[
        b"curve".as_ref(),
        &ctx.accounts.mint.key().to_bytes(),
        &[curve.bump],
    ];
    let signer = &[&seeds[..]];
    let cpi_context = CpiContext::new_with_signer(
        ctx.accounts.system_program.to_account_info(),
        anchor_lang::system_program::Transfer {
            from: ctx.accounts.sol_vault.to_account_info(),
            to: ctx.accounts.seller.to_account_info(),
        },
        signer,
    );
    anchor_lang::system_program::transfer(cpi_context, sol_out)?;

    // Update curve state
    curve.reserve_token += amount;
    curve.reserve_sol -= sol_out;

    Ok(())
}