use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::metadata::Metadata;
use anchor_spl::metadata::{
    create_metadata_accounts_v3, update_metadata_accounts_v2, CreateMetadataAccountsV3,
    UpdateMetadataAccountsV2, mpl_token_metadata::types::DataV2,
};
use anchor_spl::token::{self, Mint, Token, TokenAccount};

declare_id!("DRBGPNqVg4LwTjkj7eP2SN4d2Lrat9cmhbUZFze3fy3e");

const MAX_NAME_LEN: usize = 50;
const MAX_SYMBOL_LEN: usize = 10;
const MAX_URI_LEN: usize = 200;
const PRECISION: u128 = 1_000_000_000_000;

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
        let clock = Clock::get()?;

        // CORRECTED: Safe downcasting from u128 to u64
        let expected_raise = u64::try_from(
            (total_tokens as u128)
                .checked_mul(token_price_usdc as u128)
                .ok_or(ErrorCode::Overflow)?
        ).map_err(|_| ErrorCode::Overflow)?;
        
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
        require!(rental_yield_bps <= 10000, ErrorCode::InvalidAmount);

        let authority_key = ctx.accounts.authority.key();
        let id_bytes = listing_id.to_le_bytes();
        let bump = [ctx.bumps.listing];

        let listing_seeds = &[
            b"listing".as_ref(),
            authority_key.as_ref(),
            &id_bytes,
            &bump,
        ];

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
        listing.authority = ctx.accounts.authority.key();
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

    pub fn invest(ctx: Context<Invest>, token_amount: u64) -> Result<()> {
        let clock = Clock::get()?;

        require!(ctx.accounts.listing.is_visible, ErrorCode::ListingNotVisible);
        require!(ctx.accounts.listing.status == PropertyStatus::Fundraising, ErrorCode::NotFundraising);
        require!(clock.unix_timestamp < ctx.accounts.listing.raise_deadline, ErrorCode::RaiseClosed);
        require!(token_amount >= ctx.accounts.listing.min_investment_tokens, ErrorCode::BelowMinimum);
        
        require!(
            ctx.accounts.listing.tokens_sold.checked_add(token_amount).ok_or(ErrorCode::Overflow)? <= ctx.accounts.listing.total_tokens,
            ErrorCode::InsufficientTokens
        );

        let usdc_cost = (token_amount as u128)
            .checked_mul(ctx.accounts.listing.token_price_usdc as u128)
            .ok_or(ErrorCode::Overflow)? as u64;

        {
            let position = &mut ctx.accounts.investor_position;
            let listing = &ctx.accounts.listing;

            if position.tokens_held > 0 {
                let pending = (position.tokens_held as u128)
                    .checked_mul(
                        listing.reward_per_token_stored
                            .checked_sub(position.reward_per_token_paid)
                            .ok_or(ErrorCode::Underflow)?,
                    )
                    .ok_or(ErrorCode::Overflow)?
                    .checked_div(PRECISION)
                    .ok_or(ErrorCode::Overflow)? as u64;

                position.pending_rental_income = position.pending_rental_income.checked_add(pending).ok_or(ErrorCode::Overflow)?;
            }
            position.reward_per_token_paid = listing.reward_per_token_stored;
        }

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(), 
                token::Transfer {
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
        position.tokens_held = position.tokens_held.checked_add(token_amount).ok_or(ErrorCode::Overflow)?;
        position.usdc_invested = position.usdc_invested.checked_add(usdc_cost).ok_or(ErrorCode::Overflow)?;
        position.bump = ctx.bumps.investor_position;

        let listing = &mut ctx.accounts.listing;
        listing.tokens_sold = listing.tokens_sold.checked_add(token_amount).ok_or(ErrorCode::Overflow)?;
        listing.total_raised = listing.total_raised.checked_add(usdc_cost).ok_or(ErrorCode::Overflow)?;
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

    pub fn release_escrow(ctx: Context<ReleaseEscrow>) -> Result<()> {
        require!(ctx.accounts.listing.status == PropertyStatus::Funded, ErrorCode::RaiseTargetNotMet);
        
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
                token::Transfer {
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
                token::Transfer {
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

        listing.tokens_sold = listing.tokens_sold.checked_sub(tokens_to_burn).ok_or(ErrorCode::Underflow)?;
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

    pub fn fund_rental_vault(ctx: Context<FundRentalVault>, amount: u64) -> Result<()> {
        require!(ctx.accounts.listing.status == PropertyStatus::Active, ErrorCode::PropertyNotActive);
        require!(amount > 0, ErrorCode::InvalidAmount);

        let tokens_in_circulation = ctx.accounts.listing.tokens_sold;
        require!(tokens_in_circulation > 0, ErrorCode::InvalidAmount);

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(), 
                token::Transfer {
                    from: ctx.accounts.authority_usdc_account.to_account_info(),
                    to: ctx.accounts.rental_vault.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        let delta = (amount as u128)
            .checked_mul(PRECISION)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(tokens_in_circulation as u128)
            .ok_or(ErrorCode::Overflow)?;

        let listing = &mut ctx.accounts.listing;
        listing.reward_per_token_stored = listing.reward_per_token_stored.checked_add(delta).ok_or(ErrorCode::Overflow)?;
        listing.last_update_time = Clock::get()?.unix_timestamp;

        emit!(RentalVaultFundedEvent {
            listing_id: listing.listing_id,
            amount,
            new_reward_accumulator: listing.reward_per_token_stored,
        });

        Ok(())
    }

    pub fn claim_rental_income(ctx: Context<ClaimRentalIncome>) -> Result<()> {
        require!(ctx.accounts.listing.status == PropertyStatus::Active, ErrorCode::PropertyNotActive);

        let listing = &ctx.accounts.listing;
        let position = &mut ctx.accounts.investor_position;

        require!(position.tokens_held > 0, ErrorCode::NoPendingIncome);

        let newly_accrued = (position.tokens_held as u128)
            .checked_mul(
                listing.reward_per_token_stored
                    .checked_sub(position.reward_per_token_paid)
                    .ok_or(ErrorCode::Underflow)?,
            )
            .ok_or(ErrorCode::Overflow)?
            .checked_div(PRECISION)
            .ok_or(ErrorCode::Overflow)? as u64;

        let total_claimable = position.pending_rental_income.checked_add(newly_accrued).ok_or(ErrorCode::Overflow)?;
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
                token::Transfer {
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
            ctx.accounts.listing.status == PropertyStatus::Active || ctx.accounts.listing.status == PropertyStatus::Completed,
            ErrorCode::PropertyNotActive
        );

        let position = &mut ctx.accounts.investor_position;
        require!(!position.redemption_requested, ErrorCode::RedemptionAlreadyRequested);
        require!(position.tokens_held >= ctx.accounts.listing.unit_threshold_tokens, ErrorCode::RedemptionThresholdNotMet);

        position.redemption_requested = true;

        emit!(RedemptionRequestedEvent {
            listing_id: ctx.accounts.listing.listing_id,
            investor: ctx.accounts.investor.key(),
            tokens: position.tokens_held,
        });

        Ok(())
    }

    pub fn approve_redemption(ctx: Context<ApproveRedemption>) -> Result<()> {
        let position = &ctx.accounts.investor_position;

        require!(position.redemption_requested, ErrorCode::InvalidStatus);
        require!(!position.redemption_approved, ErrorCode::InvalidStatus);
        require!(position.tokens_held >= ctx.accounts.listing.unit_threshold_tokens, ErrorCode::RedemptionThresholdNotMet);

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
        listing.tokens_sold = listing.tokens_sold.checked_sub(tokens_to_burn).ok_or(ErrorCode::Underflow)?;
        listing.last_update_time = Clock::get()?.unix_timestamp;

        let position: &mut Account<'_, InvestorPosition> = &mut ctx.accounts.investor_position;
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
        require!(ctx.accounts.listing.status == PropertyStatus::Fundraising, ErrorCode::InvalidStatus);
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
}

// --- EVENTS ---
#[event]
pub struct ListingCreatedEvent {
    pub listing_id: u64,
    pub authority: Pubkey,
    pub name: String,
    pub raise_target: u64,
}

#[event]
pub struct InvestmentMadeEvent {
    pub listing_id: u64,
    pub investor: Pubkey,
    pub tokens_bought: u64,
    pub usdc_invested: u64,
}

#[event]
pub struct EscrowReleasedEvent {
    pub listing_id: u64,
    pub amount: u64,
}

#[event]
pub struct RefundClaimedEvent {
    pub listing_id: u64,
    pub investor: Pubkey,
    pub usdc_refunded: u64,
}

#[event]
pub struct RentalVaultFundedEvent {
    pub listing_id: u64,
    pub amount: u64,
    pub new_reward_accumulator: u128,
}

#[event]
pub struct RentalIncomeClaimedEvent {
    pub listing_id: u64,
    pub investor: Pubkey,
    pub amount: u64,
}

#[event]
pub struct RedemptionRequestedEvent {
    pub listing_id: u64,
    pub investor: Pubkey,
    pub tokens: u64,
}

#[event]
pub struct RedemptionApprovedEvent {
    pub listing_id: u64,
    pub investor: Pubkey,
    pub tokens_burned: u64,
}

#[event]
pub struct ListingCancelledEvent {
    pub listing_id: u64,
}

#[event]
pub struct MetadataUpdatedEvent {
    pub listing_id: u64,
    pub new_uri: String,
}

// --- ACCOUNTS DATA ---
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
    pub const SPACE: usize = 
        32 + 8 + (4 + MAX_NAME_LEN) + (4 + MAX_SYMBOL_LEN) + (4 + MAX_URI_LEN) 
        + 32 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 2 + 1 + 1 + 1 + 1 + 16 + 8 + 1;
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

// --- CONTEXT STRUCTS ---
#[derive(Accounts)]
#[instruction(listing_id: u64)]
pub struct CreateListing<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + PropertyListing::SPACE,
        seeds = [b"listing", authority.key().as_ref(), listing_id.to_le_bytes().as_ref()],
        bump
    )]
    pub listing: Account<'info, PropertyListing>,

    #[account(
        init,
        payer = authority,
        seeds = [b"mint", listing.key().as_ref()],
        bump,
        mint::decimals = 0,
        mint::authority = listing,
    )]
    pub property_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"metadata", metadata_program.key().as_ref(), property_mint.key().as_ref()],
        bump,
    )]
    /// CHECK: Metaplex metadata account
    pub metadata: UncheckedAccount<'info>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = usdc_mint,
        associated_token::authority = listing,
    )]
    pub escrow_vault: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        seeds = [b"rental_vault", listing.key().as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = listing,
    )]
    pub rental_vault: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: Metadata program is optional for local testing.
    pub metadata_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Invest<'info> {
    #[account(mut)]
    pub investor: Signer<'info>,
    #[account(
        mut,
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
        constraint = listing.usdc_mint == usdc_mint.key() @ ErrorCode::Unauthorized,
    )]
    pub listing: Box<Account<'info, PropertyListing>>,
    #[account(
        mut,
        constraint = investor_usdc_account.owner == investor.key() @ ErrorCode::Unauthorized,
        constraint = investor_usdc_account.mint == usdc_mint.key() @ ErrorCode::Unauthorized,
    )]
    pub investor_usdc_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = escrow_vault.key() == listing.escrow_vault @ ErrorCode::Unauthorized)]
    pub escrow_vault: Account<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = investor,
        associated_token::mint = property_mint,
        associated_token::authority = investor,
    )]
    pub investor_property_token_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = property_mint.key() == listing.property_mint @ ErrorCode::Unauthorized)]
    pub property_mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = investor,
        space = 8 + InvestorPosition::SPACE,
        seeds = [b"position", investor.key().as_ref(), listing.key().as_ref()],
        bump,
    )]
    pub investor_position: Account<'info, InvestorPosition>,
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReleaseEscrow<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
        has_one = authority @ ErrorCode::Unauthorized,
    )]
    pub listing: Account<'info, PropertyListing>,
    #[account(mut, constraint = escrow_vault.key() == listing.escrow_vault @ ErrorCode::Unauthorized)]
    pub escrow_vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = authority_usdc_account.owner == authority.key() @ ErrorCode::Unauthorized,
        constraint = authority_usdc_account.mint == listing.usdc_mint @ ErrorCode::Unauthorized,
    )]
    pub authority_usdc_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimRefund<'info> {
    #[account(mut)]
    pub investor: Signer<'info>,
    #[account(
        mut,
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
    )]
    pub listing: Box<Account<'info, PropertyListing>>,
    #[account(mut, constraint = escrow_vault.key() == listing.escrow_vault @ ErrorCode::Unauthorized)]
    pub escrow_vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = investor_usdc_account.owner == investor.key() @ ErrorCode::Unauthorized,
        constraint = investor_usdc_account.mint == listing.usdc_mint @ ErrorCode::Unauthorized,
    )]
    pub investor_usdc_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = investor_property_token_account.owner == investor.key() @ ErrorCode::Unauthorized,
        constraint = investor_property_token_account.mint == listing.property_mint @ ErrorCode::Unauthorized,
    )]
    pub investor_property_token_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = property_mint.key() == listing.property_mint @ ErrorCode::Unauthorized)]
    pub property_mint: Account<'info, Mint>,
    #[account(
        mut,
        seeds = [b"position", investor.key().as_ref(), listing.key().as_ref()],
        bump = investor_position.bump,
        constraint = investor_position.investor == investor.key() @ ErrorCode::Unauthorized,
    )]
    pub investor_position: Account<'info, InvestorPosition>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct FundRentalVault<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
        has_one = authority @ ErrorCode::Unauthorized,
    )]
    pub listing: Account<'info, PropertyListing>,
    #[account(
        mut,
        constraint = authority_usdc_account.owner == authority.key() @ ErrorCode::Unauthorized,
        constraint = authority_usdc_account.mint == listing.usdc_mint @ ErrorCode::Unauthorized,
    )]
    pub authority_usdc_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = rental_vault.key() == listing.rental_vault @ ErrorCode::Unauthorized)]
    pub rental_vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimRentalIncome<'info> {
    #[account(mut)]
    pub investor: Signer<'info>,
    #[account(
        mut,
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
    )]
    pub listing: Account<'info, PropertyListing>,
    #[account(mut, constraint = rental_vault.key() == listing.rental_vault @ ErrorCode::Unauthorized)]
    pub rental_vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = investor_usdc_account.owner == investor.key() @ ErrorCode::Unauthorized,
        constraint = investor_usdc_account.mint == listing.usdc_mint @ ErrorCode::Unauthorized,
    )]
    pub investor_usdc_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"position", investor.key().as_ref(), listing.key().as_ref()],
        bump = investor_position.bump,
        constraint = investor_position.investor == investor.key() @ ErrorCode::Unauthorized,
    )]
    pub investor_position: Account<'info, InvestorPosition>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct RequestRedemption<'info> {
    #[account(mut)]
    pub investor: Signer<'info>,
    #[account(
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
    )]
    pub listing: Account<'info, PropertyListing>,
    #[account(
        mut,
        seeds = [b"position", investor.key().as_ref(), listing.key().as_ref()],
        bump = investor_position.bump,
        constraint = investor_position.investor == investor.key() @ ErrorCode::Unauthorized,
    )]
    pub investor_position: Account<'info, InvestorPosition>,
}

#[derive(Accounts)]
pub struct ApproveRedemption<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
        has_one = authority @ ErrorCode::Unauthorized,
    )]
    pub listing: Account<'info, PropertyListing>,
    pub investor: Signer<'info>,
    #[account(
        mut,
        seeds = [b"position", investor.key().as_ref(), listing.key().as_ref()],
        bump = investor_position.bump,
        constraint = investor_position.investor == investor.key() @ ErrorCode::Unauthorized,
    )]
    pub investor_position: Account<'info, InvestorPosition>,
    #[account(
        mut,
        constraint = investor_property_token_account.owner == investor.key() @ ErrorCode::Unauthorized,
        constraint = investor_property_token_account.mint == listing.property_mint @ ErrorCode::Unauthorized,
    )]
    pub investor_property_token_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = property_mint.key() == listing.property_mint @ ErrorCode::Unauthorized)]
    pub property_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CancelListing<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
        has_one = authority @ ErrorCode::Unauthorized,
    )]
    pub listing: Account<'info, PropertyListing>,
}

#[derive(Accounts)]
pub struct UpdateMetadataUri<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"listing", listing.authority.as_ref(), listing.listing_id.to_le_bytes().as_ref()],
        bump = listing.bump,
        has_one = authority @ ErrorCode::Unauthorized,
    )]
    pub listing: Account<'info, PropertyListing>,
    #[account(
        mut,
        seeds = [b"metadata", metadata_program.key().as_ref(), listing.property_mint.as_ref()],
        bump,
    )]
    /// CHECK: Metaplex metadata account
    pub metadata: UncheckedAccount<'info>,
    /// CHECK: Metadata program is optional for local testing.
    pub metadata_program: UncheckedAccount<'info>,
}

// --- ERRORS ---
#[error_code]
pub enum ErrorCode {
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Invalid timestamp — deadline must be in the future")]
    InvalidTimestamp,
    #[msg("Invalid URI — must be between 10 and 200 characters and valid scheme")]
    InvalidUri,
    #[msg("Below minimum investment")]
    BelowMinimum,
    #[msg("Name too long — maximum 50 characters")]
    NameTooLong,
    #[msg("Symbol too long — maximum 10 characters")]
    SymbolTooLong,
    #[msg("Raise target already met")]
    RaiseTargetMet,
    #[msg("Raise deadline has passed")]
    RaiseClosed,
    #[msg("Raise target not met — cannot release escrow")]
    RaiseTargetNotMet,
    #[msg("Property is not active")]
    PropertyNotActive,
    #[msg("Property is not in fundraising status")]
    NotFundraising,
    #[msg("Insufficient tokens remaining in listing")]
    InsufficientTokens,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Arithmetic underflow")]
    Underflow,
    #[msg("No pending rental income to claim")]
    NoPendingIncome,
    #[msg("Token balance below unit redemption threshold")]
    RedemptionThresholdNotMet,
    #[msg("Redemption already requested for this position")]
    RedemptionAlreadyRequested,
    #[msg("Escrow already released to sponsor")]
    EscrowAlreadyReleased,
    #[msg("Raise is still active — deadline not yet passed")]
    RaiseStillActive,
    #[msg("Property listing is not visible")]
    ListingNotVisible,
    #[msg("Invalid property status for this operation")]
    InvalidStatus,
}