use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Invalid timestamp - deadline must be in the future")]
    InvalidTimestamp,
    #[msg("Invalid URI - must be between 10 and 200 characters and valid scheme")]
    InvalidUri,
    #[msg("Below minimum investment")]
    BelowMinimum,
    #[msg("Name too long - maximum 50 characters")]
    NameTooLong,
    #[msg("Symbol too long - maximum 10 characters")]
    SymbolTooLong,
    #[msg("Raise target already met")]
    RaiseTargetMet,
    #[msg("Raise deadline has passed")]
    RaiseClosed,
    #[msg("Raise target not met - cannot release escrow")]
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
    #[msg("Raise is still active - deadline not yet passed")]
    RaiseStillActive,
    #[msg("Property listing is not visible")]
    ListingNotVisible,
    #[msg("Invalid property status for this operation")]
    InvalidStatus,
}
