use anchor_lang::prelude::*;

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
