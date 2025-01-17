use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};
use crate::state::Curve;

#[derive(Accounts)]
pub struct InitializeLaunch<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(
        init,
        payer = creator,
        space = 8 + Curve::LEN,
        seeds = [b"curve".as_ref(), mint.key().as_ref()],
        bump
    )]
    pub curve: Account<'info, Curve>,
    pub mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<InitializeLaunch>, total_supply: u64, curve_type: u8, custom_params: [u64; 3]) -> Result<()> {
    let curve = &mut ctx.accounts.curve;
    curve.initialize(
        ctx.accounts.creator.key(),
        ctx.accounts.mint.key(),
        total_supply,
        curve_type,
        custom_params,
        *ctx.bumps.get("curve").unwrap(),
    )
}