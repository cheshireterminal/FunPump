use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use std::convert::TryInto;

declare_id!("YourProgramID");

// Constants
pub const SECONDS_IN_DAY: i64 = 86400;
pub const MINIMUM_VESTING_PERIOD: i64 = SECONDS_IN_DAY * 7; // 1 week
pub const MAXIMUM_VESTING_PERIOD: i64 = SECONDS_IN_DAY * 365 * 2; // 2 years
pub const MINIMUM_AMOUNT: u64 = 1;
pub const BASIS_POINTS: u16 = 10000; // For percentage calculations

// Enums
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

// Account Structures
#[account]
pub struct TokenLaunch {
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub total_supply: u64,
    pub curve: Curve,
    pub vesting_config: VestingConfig,
    pub stream_config: StreamConfig,
    pub market_caps: Vec<MarketCapMilestone>,
    pub is_initialized: bool,
    pub bump: u8,
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

#[account]
pub struct VestingConfig {
    pub schedule_type: VestingScheduleType,
    pub start_time: i64,
    pub end_time: i64,
    pub cliff_period: i64,
    pub total_amount: u64,
    pub released_amount: u64,
    pub milestones: Vec<VestingMilestone>,
}

#[account]
pub struct StreamConfig {
    pub stream_type: StreamType,
    pub rate: u64,
    pub interval: i64,
    pub last_update_time: i64,
    pub total_streamed: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MarketCapMilestone {
    pub target_cap: u64,
    pub unlock_percentage: u8,
    pub is_reached: bool,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct VestingMilestone {
    pub time: i64,
    pub percentage: u8,
    pub is_claimed: bool,
}

// Result Structures
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct BuyResult {
    pub token_amount: u64,
    pub sol_amount: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SellResult {
    pub token_amount: u64,
    pub sol_amount: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct StreamResult {
    pub amount: u64,
    pub timestamp: i64,
}

// Error Codes
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

// Curve Implementation
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

    // Linear Bonding Curve
    fn calculate_linear_buy_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let slope = self.custom_params[0] as u128;

        let price = (virtual_sol * amount) / virtual_token;
        let linear_factor = (amount * slope) / BASIS_POINTS as u128;

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

        let base_price = (virtual_sol * amount) / virtual_token;
        let linear_factor = (amount * slope) / BASIS_POINTS as u128;

        let total_price = base_price
            .checked_sub(linear_factor)
            .ok_or(CustomError::CalculationError)?;

        Ok((total_price as u64).min(self.real_sol_reserves))
    }

    // Exponential Bonding Curve
    fn calculate_exponential_buy_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let exponent = self.custom_params[1] as u128;

        let base_price = (virtual_sol * amount) / virtual_token;
        let exp_factor = ((amount * exponent) / BASIS_POINTS as u128).pow(2);

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

        let base_price = (virtual_sol * amount) / virtual_token;
        let exp_factor = ((amount * exponent) / BASIS_POINTS as u128).pow(2);

        let total_price = base_price
            .checked_sub(exp_factor)
            .ok_or(CustomError::CalculationError)?;

        Ok((total_price as u64).min(self.real_sol_reserves))
    }

    // Sigmoid Bonding Curve
    fn calculate_sigmoid_buy_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let midpoint = self.custom_params[2] as u128;

        let x = (amount * BASIS_POINTS as u128) / virtual_token;
        let sigmoid = self.sigmoid(x, midpoint)?;

        let price = (virtual_sol * amount * sigmoid) / (virtual_token * BASIS_POINTS as u128);

        Ok(price as u64)
    }

    fn calculate_sigmoid_sell_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let midpoint = self.custom_params[2] as u128;

        let x = (amount * BASIS_POINTS as u128) / virtual_token;
        let sigmoid = self.sigmoid(x, midpoint)?;

        let price = (virtual_sol * amount * sigmoid) / (virtual_token * BASIS_POINTS as u128);

        Ok((price as u64).min(self.real_sol_reserves))
    }

    // Custom Bonding Curve
    fn calculate_custom_buy_price(&self, amount: u64) -> Result<u64> {
        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;

        let slope = self.custom_params[0] as u128;
        let exponent = self.custom_params[1] as u128;
        let midpoint = self.custom_params[2] as u128;

        let base_price = (virtual_sol * amount) / virtual_token;
        let custom_factor = (amount * slope * exponent) / (midpoint * BASIS_POINTS as u128);

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

        let base_price = (virtual_sol * amount) / virtual_token;
        let custom_factor = (amount * slope * exponent) / (midpoint * BASIS_POINTS as u128);

        let total_price = base_price
            .checked_sub(custom_factor)
            .ok_or(CustomError::CalculationError)?;

        Ok((total_price as u64).min(self.real_sol_reserves))
    }

    // Helper Functions
    fn sigmoid(&self, x: u128, midpoint: u128) -> Result<u128> {
        let numerator = x.checked_mul(BASIS_POINTS as u128)
            .ok_or(CustomError::CalculationError)?;

        let denominator = x.checked_add(midpoint)
            .ok_or(CustomError::CalculationError)?;

        Ok(numerator / denominator)
    }

    pub fn update_reserves(&mut self, sol_delta: i64, token_delta: i64) -> Result<()> {
        if sol_delta > 0 {
            self.real_sol_reserves = self.real_sol_reserves
                .checked_add(sol_delta as u64)
                .ok_or(CustomError::CalculationError)?;
        } else {
            self.real_sol_reserves = self.real_sol_reserves
                .checked_sub(sol_delta.abs() as u64)
                .ok_or(CustomError::CalculationError)?;
        }

        if token_delta > 0 {
            self.real_token_reserves = self.real_token_reserves
                .checked_add(token_delta as u64)
                .ok_or(CustomError::CalculationError)?;
        } else {
            self.real_token_reserves = self.real_token_reserves
                .checked_sub(token_delta.abs() as u64)
                .ok_or(CustomError::CalculationError)?;
        }

        Ok(())
    }
}

// Vesting Implementation
impl VestingConfig {
    pub fn initialize(
        &mut self,
        schedule_type: VestingScheduleType,
        start_time: i64,
        end_time: i64,
        cliff_period: i64,
        total_amount: u64,
        milestones: Vec<VestingMilestone>,
    ) -> Result<()> {
        require!(end_time > start_time, CustomError::InvalidTimeParameters);
        require!(cliff_period >= 0, CustomError::InvalidTimeParameters);
        require!(total_amount > 0, CustomError::InvalidAmount);

        self.schedule_type = schedule_type;
        self.start_time = start_time;
        self.end_time = end_time;
        self.cliff_period = cliff_period;
        self.total_amount = total_amount;
        self.released_amount = 0;

        if schedule_type == VestingScheduleType::CustomMilestone {
            require!(!milestones.is_empty(), CustomError::InvalidMilestone);
            let total_percentage: u16 = milestones.iter()
                .map(|m| m.percentage as u16)
                .sum();
            require!(total_percentage == 100, CustomError::InvalidMilestone);
        }

        self.milestones = milestones;
        Ok(())
    }

    pub fn calculate_vested_amount(&self, current_time: i64) -> Result<u64> {
        if current_time < self.start_time + self.cliff_period {
            return Ok(0);
        }

        let vested_amount = match self.schedule_type {
            VestingScheduleType::Linear => self.calculate_linear_vesting(current_time),
            VestingScheduleType::Cliff => self.calculate_cliff_vesting(current_time),
            VestingScheduleType::Staggered => self.calculate_staggered_vesting(current_time),
            VestingScheduleType::CustomMilestone => self.calculate_milestone_vesting(current_time),
        }?;

        Ok(vested_amount.min(self.total_amount))
    }

    fn calculate_linear_vesting(&self, current_time: i64) -> Result<u64> {
        if current_time >= self.end_time {
            return Ok(self.total_amount);
        }

        let total_duration = self.end_time.checked_sub(self.start_time)
            .ok_or(CustomError::CalculationError)?;
        let elapsed_time = current_time.checked_sub(self.start_time)
            .ok_or(CustomError::CalculationError)?;

        let vested_amount = (self.total_amount as u128)
            .checked_mul(elapsed_time as u128)
            .ok_or(CustomError::CalculationError)?
            .checked_div(total_duration as u128)
            .ok_or(CustomError::CalculationError)?;

        Ok(vested_amount as u64)
    }

    fn calculate_cliff_vesting(&self, current_time: i64) -> Result<u64> {
        if current_time >= self.end_time {
            Ok(self.total_amount)
        } else {
            Ok(0)
        }
    }

    fn calculate_staggered_vesting(&self, current_time: i64) -> Result<u64> {
        let total_duration = self.end_time.checked_sub(self.start_time)
            .ok_or(CustomError::CalculationError)?;
        let stages = 4; // Quarterly releases
        let stage_duration = total_duration / stages;
        let elapsed_time = current_time.checked_sub(self.start_time)
            .ok_or(CustomError::CalculationError)?;
        let current_stage = elapsed_time / stage_duration;

        let stage_amount = self.total_amount / stages as u64;
        Ok(stage_amount * current_stage.min(stages) as u64)
    }

    fn calculate_milestone_vesting(&self, current_time: i64) -> Result<u64> {
        let mut vested_amount = 0u64;

        for milestone in &self.milestones {
            if current_time >= milestone.time && !milestone.is_claimed {
                let milestone_amount = (self.total_amount as u128)
                    .checked_mul(milestone.percentage as u128)
                    .ok_or(CustomError::CalculationError)?
                    .checked_div(100)
                    .ok_or(CustomError::CalculationError)?;

                vested_amount = vested_amount
                    .checked_add(milestone_amount as u64)
                    .ok_or(CustomError::CalculationError)?;
            }
        }

        Ok(vested_amount)
    }
}

// Stream Implementation
impl StreamConfig {
    pub fn initialize(
        &mut self,
        stream_type: StreamType,
        rate: u64,
        interval: i64,
    )
