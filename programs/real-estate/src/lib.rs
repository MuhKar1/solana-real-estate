use anchor_lang::prelude::*;

pub mod constants;
pub mod contexts;
pub mod errors;
pub mod events;
pub mod admin_instructions;
pub mod investor_instructions;
pub mod state;

use contexts::*;

declare_id!("DRBGPNqVg4LwTjkj7eP2SN4d2Lrat9cmhbUZFze3fy3e");

#[program]
pub mod real_estate {
    use super::*;

    pub fn create_listing(
        ctx: Context<CreateListing>,
        listing_id: u64,
        name: String,
        symbol: String,
        uri: String,
        total_tokens: u64,
        token_price_usdc: u64,
        min_investment_tokens: u64,
        raise_target: u64,
        raise_deadline: i64,
        unit_threshold_tokens: u64,
        rental_yield_bps: u16,
        is_off_plan: bool,
        show_exact_amounts: bool,
    ) -> Result<()> {
        admin_instructions::create_listing(
            ctx,
            listing_id,
            name,
            symbol,
            uri,
            total_tokens,
            token_price_usdc,
            min_investment_tokens,
            raise_target,
            raise_deadline,
            unit_threshold_tokens,
            rental_yield_bps,
            is_off_plan,
            show_exact_amounts,
        )
    }

    pub fn invest(ctx: Context<Invest>, token_amount: u64) -> Result<()> {
        investor_instructions::invest(ctx, token_amount)
    }

    pub fn release_escrow(ctx: Context<ReleaseEscrow>) -> Result<()> {
        admin_instructions::release_escrow(ctx)
    }

    pub fn claim_refund(ctx: Context<ClaimRefund>) -> Result<()> {
        investor_instructions::claim_refund(ctx)
    }

    pub fn fund_rental_vault(ctx: Context<FundRentalVault>, amount: u64) -> Result<()> {
        admin_instructions::fund_rental_vault(ctx, amount)
    }

    pub fn claim_rental_income(ctx: Context<ClaimRentalIncome>) -> Result<()> {
        investor_instructions::claim_rental_income(ctx)
    }

    pub fn request_redemption(ctx: Context<RequestRedemption>) -> Result<()> {
        investor_instructions::request_redemption(ctx)
    }

    pub fn approve_redemption(ctx: Context<ApproveRedemption>) -> Result<()> {
        admin_instructions::approve_redemption(ctx)
    }

    pub fn cancel_listing(ctx: Context<CancelListing>) -> Result<()> {
        admin_instructions::cancel_listing(ctx)
    }

    pub fn update_metadata_uri(ctx: Context<UpdateMetadataUri>, new_uri: String) -> Result<()> {
        admin_instructions::update_metadata_uri(ctx, new_uri)
    }
}
