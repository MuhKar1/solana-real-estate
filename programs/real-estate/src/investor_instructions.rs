use anchor_lang::prelude::*;
use anchor_spl::token::{self, Transfer};

use crate::constants::PRECISION;
use crate::contexts::*;
use crate::errors::ErrorCode;
use crate::events::*;
use crate::state::PropertyStatus;

fn checked_mul_u64(a: u64, b: u64) -> Result<u64> {
    u64::try_from((a as u128).checked_mul(b as u128).ok_or(ErrorCode::Overflow)?)
        .map_err(|_| ErrorCode::Overflow.into())
}

fn calculate_pending_income(tokens_held: u64, reward_per_token_delta: u128) -> Result<u64> {
    u64::try_from(
        (tokens_held as u128)
            .checked_mul(reward_per_token_delta)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(PRECISION)
            .ok_or(ErrorCode::Overflow)?,
    )
    .map_err(|_| ErrorCode::Overflow.into())
}

pub fn invest(ctx: Context<Invest>, token_amount: u64) -> Result<()> {
    let clock = Clock::get()?;

    require!(ctx.accounts.listing.is_visible, ErrorCode::ListingNotVisible);
    require!(ctx.accounts.listing.status == PropertyStatus::Fundraising, ErrorCode::NotFundraising);
    require!(clock.unix_timestamp < ctx.accounts.listing.raise_deadline, ErrorCode::RaiseClosed);
    require!(token_amount >= ctx.accounts.listing.min_investment_tokens, ErrorCode::BelowMinimum);

    require!(
        ctx.accounts
            .listing
            .tokens_sold
            .checked_add(token_amount)
            .ok_or(ErrorCode::Overflow)?
            <= ctx.accounts.listing.total_tokens,
        ErrorCode::InsufficientTokens
    );

    let usdc_cost = checked_mul_u64(token_amount, ctx.accounts.listing.token_price_usdc)?;

    {
        let position = &mut ctx.accounts.investor_position;
        let listing = &ctx.accounts.listing;

        if position.tokens_held > 0 {
            let reward_delta = listing
                .reward_per_token_stored
                .checked_sub(position.reward_per_token_paid)
                .ok_or(ErrorCode::Underflow)?;

            let pending = calculate_pending_income(position.tokens_held, reward_delta)?;
            position.pending_rental_income = position
                .pending_rental_income
                .checked_add(pending)
                .ok_or(ErrorCode::Overflow)?;
        }

        position.reward_per_token_paid = listing.reward_per_token_stored;
    }

    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.investor_usdc_account.to_account_info(),
                to: ctx.accounts.escrow_vault.to_account_info(),
                authority: ctx.accounts.investor.to_account_info(),
            },
        ),
        usdc_cost,
    )?;

    let authority_key = ctx.accounts.listing.authority;
    let id_bytes = ctx.accounts.listing.listing_id.to_le_bytes();
    let bump = [ctx.accounts.listing.bump];
    let listing_seeds: &[&[u8]] = &[b"listing", authority_key.as_ref(), id_bytes.as_ref(), &bump];
    let signer_seeds: &[&[&[u8]]] = &[listing_seeds];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            token::MintTo {
                mint: ctx.accounts.property_mint.to_account_info(),
                to: ctx.accounts.investor_property_token_account.to_account_info(),
                authority: ctx.accounts.listing.to_account_info(),
            },
            signer_seeds,
        ),
        token_amount,
    )?;

    let position = &mut ctx.accounts.investor_position;
    position.investor = ctx.accounts.investor.key();
    position.listing = ctx.accounts.listing.key();
    position.tokens_held = position
        .tokens_held
        .checked_add(token_amount)
        .ok_or(ErrorCode::Overflow)?;
    position.usdc_invested = position
        .usdc_invested
        .checked_add(usdc_cost)
        .ok_or(ErrorCode::Overflow)?;
    position.bump = ctx.bumps.investor_position;

    let listing = &mut ctx.accounts.listing;
    listing.tokens_sold = listing
        .tokens_sold
        .checked_add(token_amount)
        .ok_or(ErrorCode::Overflow)?;
    listing.total_raised = listing
        .total_raised
        .checked_add(usdc_cost)
        .ok_or(ErrorCode::Overflow)?;
    listing.last_update_time = clock.unix_timestamp;

    if listing.total_raised >= listing.raise_target {
        listing.status = PropertyStatus::Funded;
    }

    emit!(InvestmentMadeEvent {
        listing_id: listing.listing_id,
        investor: ctx.accounts.investor.key(),
        tokens_bought: token_amount,
        usdc_invested: usdc_cost,
    });

    Ok(())
}

pub fn claim_refund(ctx: Context<ClaimRefund>) -> Result<()> {
    let clock = Clock::get()?;
    let listing_account_info = ctx.accounts.listing.to_account_info();
    let authority_key = ctx.accounts.listing.authority;
    let id_bytes = ctx.accounts.listing.listing_id.to_le_bytes();
    let bump = [ctx.accounts.listing.bump];
    let listing = &mut ctx.accounts.listing;

    require!(
        listing.status == PropertyStatus::Fundraising || listing.status == PropertyStatus::Cancelled,
        ErrorCode::InvalidStatus
    );

    if listing.status == PropertyStatus::Fundraising {
        require!(clock.unix_timestamp >= listing.raise_deadline, ErrorCode::RaiseStillActive);
    }

    let position = &ctx.accounts.investor_position;
    require!(position.usdc_invested > 0, ErrorCode::InvalidAmount);

    let refund_amount = position.usdc_invested;
    let tokens_to_burn = position.tokens_held;

    let listing_seeds: &[&[u8]] = &[b"listing", authority_key.as_ref(), id_bytes.as_ref(), &bump];
    let signer_seeds: &[&[&[u8]]] = &[listing_seeds];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.escrow_vault.to_account_info(),
                to: ctx.accounts.investor_usdc_account.to_account_info(),
                authority: listing_account_info,
            },
            signer_seeds,
        ),
        refund_amount,
    )?;

    token::burn(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            token::Burn {
                mint: ctx.accounts.property_mint.to_account_info(),
                from: ctx.accounts.investor_property_token_account.to_account_info(),
                authority: ctx.accounts.investor.to_account_info(),
            },
        ),
        tokens_to_burn,
    )?;

    listing.tokens_sold = listing
        .tokens_sold
        .checked_sub(tokens_to_burn)
        .ok_or(ErrorCode::Underflow)?;
    listing.last_update_time = clock.unix_timestamp;

    let position = &mut ctx.accounts.investor_position;
    position.usdc_invested = 0;
    position.tokens_held = 0;
    position.pending_rental_income = 0;
    position.reward_per_token_paid = 0;
    position.redemption_requested = false;
    position.redemption_approved = false;

    emit!(RefundClaimedEvent {
        listing_id: listing.listing_id,
        investor: ctx.accounts.investor.key(),
        usdc_refunded: refund_amount,
    });

    Ok(())
}

pub fn claim_rental_income(ctx: Context<ClaimRentalIncome>) -> Result<()> {
    require!(
        ctx.accounts.listing.status == PropertyStatus::Active,
        ErrorCode::PropertyNotActive
    );

    let listing = &ctx.accounts.listing;
    let position = &mut ctx.accounts.investor_position;

    require!(position.tokens_held > 0, ErrorCode::NoPendingIncome);

    let reward_delta = listing
        .reward_per_token_stored
        .checked_sub(position.reward_per_token_paid)
        .ok_or(ErrorCode::Underflow)?;

    let newly_accrued = calculate_pending_income(position.tokens_held, reward_delta)?;
    let total_claimable = position
        .pending_rental_income
        .checked_add(newly_accrued)
        .ok_or(ErrorCode::Overflow)?;

    require!(total_claimable > 0, ErrorCode::NoPendingIncome);
    require!(ctx.accounts.rental_vault.amount >= total_claimable, ErrorCode::InvalidAmount);

    position.reward_per_token_paid = listing.reward_per_token_stored;
    position.pending_rental_income = 0;

    let authority_key = listing.authority;
    let id_bytes = listing.listing_id.to_le_bytes();
    let bump = [listing.bump];
    let listing_seeds: &[&[u8]] = &[b"listing", authority_key.as_ref(), id_bytes.as_ref(), &bump];
    let signer_seeds: &[&[&[u8]]] = &[listing_seeds];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.rental_vault.to_account_info(),
                to: ctx.accounts.investor_usdc_account.to_account_info(),
                authority: ctx.accounts.listing.to_account_info(),
            },
            signer_seeds,
        ),
        total_claimable,
    )?;

    emit!(RentalIncomeClaimedEvent {
        listing_id: listing.listing_id,
        investor: ctx.accounts.investor.key(),
        amount: total_claimable,
    });

    Ok(())
}

pub fn request_redemption(ctx: Context<RequestRedemption>) -> Result<()> {
    require!(
        ctx.accounts.listing.status == PropertyStatus::Active
            || ctx.accounts.listing.status == PropertyStatus::Completed,
        ErrorCode::PropertyNotActive
    );

    let position = &mut ctx.accounts.investor_position;
    require!(!position.redemption_requested, ErrorCode::RedemptionAlreadyRequested);
    require!(
        position.tokens_held >= ctx.accounts.listing.unit_threshold_tokens,
        ErrorCode::RedemptionThresholdNotMet
    );

    position.redemption_requested = true;

    emit!(RedemptionRequestedEvent {
        listing_id: ctx.accounts.listing.listing_id,
        investor: ctx.accounts.investor.key(),
        tokens: position.tokens_held,
    });

    Ok(())
}
