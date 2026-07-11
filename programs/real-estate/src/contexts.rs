use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::errors::ErrorCode;
use crate::state::{InvestorPosition, PropertyListing};

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
