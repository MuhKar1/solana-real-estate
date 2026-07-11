use anchor_lang::prelude::*;
use anchor_spl::metadata::{
    create_metadata_accounts_v3, update_metadata_accounts_v2, CreateMetadataAccountsV3,
    UpdateMetadataAccountsV2, mpl_token_metadata::types::DataV2,
};
use anchor_spl::token::{self, Transfer};

use crate::constants::{MAX_NAME_LEN, MAX_SYMBOL_LEN, MAX_URI_LEN, PRECISION};
use crate::contexts::*;
use crate::errors::ErrorCode;
use crate::events::*;
use crate::state::PropertyStatus;

fn checked_mul_u64(a: u64, b: u64) -> Result<u64> {
    u64::try_from((a as u128).checked_mul(b as u128).ok_or(ErrorCode::Overflow)?)
        .map_err(|_| ErrorCode::Overflow.into())
}

fn calculate_reward_delta(amount: u64, tokens_in_circulation: u64) -> Result<u128> {
    require!(tokens_in_circulation > 0, ErrorCode::InvalidAmount);

    let product = (amount as u128)
        .checked_mul(PRECISION)
        .ok_or(ErrorCode::Overflow)?;

    product
        .checked_div(tokens_in_circulation as u128)
        .ok_or(ErrorCode::Overflow.into())
}

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
    let clock = Clock::get()?;

    let expected_raise = checked_mul_u64(total_tokens, token_price_usdc)?;

    require!(raise_target == expected_raise, ErrorCode::InvalidAmount);
    require!(total_tokens > 0, ErrorCode::InvalidAmount);
    require!(raise_target > 0, ErrorCode::InvalidAmount);
    require!(token_price_usdc > 0, ErrorCode::InvalidAmount);
    require!(min_investment_tokens > 0, ErrorCode::InvalidAmount);
    require!(min_investment_tokens <= total_tokens, ErrorCode::InvalidAmount);
    require!(unit_threshold_tokens <= total_tokens, ErrorCode::InvalidAmount);
    require!(raise_deadline > clock.unix_timestamp, ErrorCode::InvalidTimestamp);
    require!(name.len() > 0 && name.len() <= MAX_NAME_LEN, ErrorCode::NameTooLong);
    require!(symbol.len() > 0 && symbol.len() <= MAX_SYMBOL_LEN, ErrorCode::SymbolTooLong);
    require!(uri.len() > 10 && uri.len() <= MAX_URI_LEN, ErrorCode::InvalidUri);
    require!(uri.starts_with("https://") || uri.starts_with("ipfs://"), ErrorCode::InvalidUri);
    require!(rental_yield_bps <= 10_000, ErrorCode::InvalidAmount);

    let authority_key = ctx.accounts.authority.key();
    let id_bytes = listing_id.to_le_bytes();
    let bump = [ctx.bumps.listing];

    let listing_seeds = &[b"listing".as_ref(), authority_key.as_ref(), &id_bytes, &bump];

    if ctx.accounts.metadata_program.to_account_info().executable {
        create_metadata_accounts_v3(
            CpiContext::new_with_signer(
                ctx.accounts.metadata_program.key(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.metadata.to_account_info(),
                    mint: ctx.accounts.property_mint.to_account_info(),
                    mint_authority: ctx.accounts.listing.to_account_info(),
                    payer: ctx.accounts.authority.to_account_info(),
                    update_authority: ctx.accounts.listing.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
                &[listing_seeds],
            ),
            DataV2 {
                name: name.clone(),
                symbol: symbol.clone(),
                uri: uri.clone(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            true,
            true,
            None,
        )?;
    }

    let listing = &mut ctx.accounts.listing;
    listing.authority = authority_key;
    listing.listing_id = listing_id;
    listing.name = name;
    listing.symbol = symbol;
    listing.uri = uri;
    listing.property_mint = ctx.accounts.property_mint.key();
    listing.escrow_vault = ctx.accounts.escrow_vault.key();
    listing.rental_vault = ctx.accounts.rental_vault.key();
    listing.usdc_mint = ctx.accounts.usdc_mint.key();
    listing.total_tokens = total_tokens;
    listing.tokens_sold = 0;
    listing.total_raised = 0;
    listing.token_price_usdc = token_price_usdc;
    listing.min_investment_tokens = min_investment_tokens;
    listing.raise_target = raise_target;
    listing.raise_deadline = raise_deadline;
    listing.unit_threshold_tokens = unit_threshold_tokens;
    listing.rental_yield_bps = rental_yield_bps;
    listing.is_off_plan = is_off_plan;
    listing.show_exact_amounts = show_exact_amounts;
    listing.status = PropertyStatus::Fundraising;
    listing.is_visible = true;
    listing.reward_per_token_stored = 0;
    listing.last_update_time = clock.unix_timestamp;
    listing.bump = ctx.bumps.listing;

    emit!(ListingCreatedEvent {
        listing_id,
        authority: authority_key,
        name: listing.name.clone(),
        raise_target,
    });

    Ok(())
}

pub fn release_escrow(ctx: Context<ReleaseEscrow>) -> Result<()> {
    require!(
        ctx.accounts.listing.status == PropertyStatus::Funded,
        ErrorCode::RaiseTargetNotMet
    );

    let amount = ctx.accounts.escrow_vault.amount;
    require!(amount > 0, ErrorCode::InvalidAmount);

    let authority_key = ctx.accounts.listing.authority;
    let id_bytes = ctx.accounts.listing.listing_id.to_le_bytes();
    let bump = [ctx.accounts.listing.bump];
    let listing_seeds: &[&[u8]] = &[b"listing", authority_key.as_ref(), id_bytes.as_ref(), &bump];
    let signer_seeds: &[&[&[u8]]] = &[listing_seeds];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.escrow_vault.to_account_info(),
                to: ctx.accounts.authority_usdc_account.to_account_info(),
                authority: ctx.accounts.listing.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )?;

    let listing = &mut ctx.accounts.listing;
    listing.status = PropertyStatus::Active;
    listing.last_update_time = Clock::get()?.unix_timestamp;

    emit!(EscrowReleasedEvent {
        listing_id: listing.listing_id,
        amount,
    });

    Ok(())
}

pub fn fund_rental_vault(ctx: Context<FundRentalVault>, amount: u64) -> Result<()> {
    require!(
        ctx.accounts.listing.status == PropertyStatus::Active,
        ErrorCode::PropertyNotActive
    );
    require!(amount > 0, ErrorCode::InvalidAmount);

    let tokens_in_circulation = ctx.accounts.listing.tokens_sold;

    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.authority_usdc_account.to_account_info(),
                to: ctx.accounts.rental_vault.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        amount,
    )?;

    let delta = calculate_reward_delta(amount, tokens_in_circulation)?;

    let listing = &mut ctx.accounts.listing;
    listing.reward_per_token_stored = listing
        .reward_per_token_stored
        .checked_add(delta)
        .ok_or(ErrorCode::Overflow)?;
    listing.last_update_time = Clock::get()?.unix_timestamp;

    emit!(RentalVaultFundedEvent {
        listing_id: listing.listing_id,
        amount,
        new_reward_accumulator: listing.reward_per_token_stored,
    });

    Ok(())
}

pub fn approve_redemption(ctx: Context<ApproveRedemption>) -> Result<()> {
    let position = &ctx.accounts.investor_position;

    require!(position.redemption_requested, ErrorCode::InvalidStatus);
    require!(!position.redemption_approved, ErrorCode::InvalidStatus);
    require!(
        position.tokens_held >= ctx.accounts.listing.unit_threshold_tokens,
        ErrorCode::RedemptionThresholdNotMet
    );

    let tokens_to_burn = position.tokens_held;

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

    let listing = &mut ctx.accounts.listing;
    listing.tokens_sold = listing
        .tokens_sold
        .checked_sub(tokens_to_burn)
        .ok_or(ErrorCode::Underflow)?;
    listing.last_update_time = Clock::get()?.unix_timestamp;

    let position = &mut ctx.accounts.investor_position;
    position.tokens_held = 0;
    position.usdc_invested = 0;
    position.pending_rental_income = 0;
    position.reward_per_token_paid = 0;
    position.redemption_requested = false;
    position.redemption_approved = true;

    emit!(RedemptionApprovedEvent {
        listing_id: listing.listing_id,
        investor: ctx.accounts.investor.key(),
        tokens_burned: tokens_to_burn,
    });

    Ok(())
}

pub fn cancel_listing(ctx: Context<CancelListing>) -> Result<()> {
    require!(
        ctx.accounts.listing.status == PropertyStatus::Fundraising,
        ErrorCode::InvalidStatus
    );

    let listing = &mut ctx.accounts.listing;
    listing.status = PropertyStatus::Cancelled;
    listing.last_update_time = Clock::get()?.unix_timestamp;

    emit!(ListingCancelledEvent {
        listing_id: listing.listing_id,
    });

    Ok(())
}

pub fn update_metadata_uri(ctx: Context<UpdateMetadataUri>, new_uri: String) -> Result<()> {
    require!(new_uri.len() > 10 && new_uri.len() <= MAX_URI_LEN, ErrorCode::InvalidUri);
    require!(new_uri.starts_with("https://") || new_uri.starts_with("ipfs://"), ErrorCode::InvalidUri);

    let listing_info = ctx.accounts.listing.to_account_info();
    let listing = &mut ctx.accounts.listing;
    listing.uri = new_uri.clone();

    let authority_key = ctx.accounts.authority.key();
    let id_bytes = listing.listing_id.to_le_bytes();
    let bump = [listing.bump];
    let listing_seeds = &[b"listing".as_ref(), authority_key.as_ref(), &id_bytes, &bump];

    if ctx.accounts.metadata_program.to_account_info().executable {
        update_metadata_accounts_v2(
            CpiContext::new_with_signer(
                ctx.accounts.metadata_program.key(),
                UpdateMetadataAccountsV2 {
                    metadata: ctx.accounts.metadata.to_account_info(),
                    update_authority: listing_info,
                },
                &[listing_seeds],
            ),
            None,
            Some(DataV2 {
                name: listing.name.clone(),
                symbol: listing.symbol.clone(),
                uri: new_uri.clone(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            }),
            None,
            Some(true),
        )?;
    }

    emit!(MetadataUpdatedEvent {
        listing_id: listing.listing_id,
        new_uri,
    });

    Ok(())
}
