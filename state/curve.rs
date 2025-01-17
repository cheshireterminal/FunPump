use anchor_lang::prelude::*;

#[account]
pub struct Curve {
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub total_supply: u64,
    pub reserve_token: u64,
    pub reserve_sol: u64,
    pub curve_type: u8,
    pub custom_params: [u64; 3],
    pub bump: u8,
}

impl Curve {
    pub const LEN: usize = 8 + 32 + 32 + 8 + 8 + 8 + 1 + 24 + 1;
}