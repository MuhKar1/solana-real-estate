use anchor_lang::prelude::*;

use crate::constants::{MAX_NAME_LEN, MAX_SYMBOL_LEN, MAX_URI_LEN};

#[account]
pub struct PropertyListing {
    pub authority: Pubkey,
    pub listing_id: u64,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub property_mint: Pubkey,
    pub escrow_vault: Pubkey,
    pub rental_vault: Pubkey,
    pub usdc_mint: Pubkey,
    pub total_tokens: u64,
    pub tokens_sold: u64,
    pub total_raised: u64,
    pub token_price_usdc: u64,
    pub min_investment_tokens: u64,
    pub raise_target: u64,
    pub raise_deadline: i64,
    pub unit_threshold_tokens: u64,
    pub rental_yield_bps: u16,
    pub is_off_plan: bool,
    pub show_exact_amounts: bool,
    pub status: PropertyStatus,
    pub is_visible: bool,
    pub reward_per_token_stored: u128,
    pub last_update_time: i64,
    pub bump: u8,
}

impl PropertyListing {
    pub const SPACE: usize = 32
        + 8
        + (4 + MAX_NAME_LEN)
        + (4 + MAX_SYMBOL_LEN)
        + (4 + MAX_URI_LEN)
        + 32
        + 32
        + 32
        + 32
        + 8
        + 8
        + 8
        + 8
        + 8
        + 8
        + 8
        + 8
        + 2
        + 1
        + 1
        + 1
        + 1
        + 16
        + 8
        + 1;
}

#[account]
#[derive(Default)]
pub struct InvestorPosition {
    pub investor: Pubkey,
    pub listing: Pubkey,
    pub tokens_held: u64,
    pub usdc_invested: u64,
    pub reward_per_token_paid: u128,
    pub pending_rental_income: u64,
    pub redemption_requested: bool,
    pub redemption_approved: bool,
    pub bump: u8,
}

impl InvestorPosition {
    pub const SPACE: usize = 32 + 32 + 8 + 8 + 16 + 8 + 1 + 1 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Default)]
pub enum PropertyStatus {
    #[default]
    Draft,
    Fundraising,
    Funded,
    Active,
    Completed,
    Cancelled,
}
