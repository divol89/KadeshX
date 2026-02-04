use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

declare_id!("EsuD4bWYtzJmHJpzjqo9xXPDCJwoNzrK5BsGPw83yiE5");

pub const DECIMALS: u32 = 9;
pub const SECONDS_PER_MONTH: i64 = 30 * 24 * 60 * 60;

/// Maximum allowed TGE percentage (100% = 10000 bps)
pub const MAX_TGE_PERCENTAGE: u64 = 10000;
/// Minimum vesting duration to prevent instant unlock
pub const MIN_VESTING_DURATION: i64 = 1;

// Miktarlar (Aynen korundu)
pub const CASHBACK_AMOUNT: u64 = 500_000_000_000_000_000;
pub const PRESALE_AMOUNT: u64 = 100_000_000_000_000_000;
pub const LIQUIDITY_AMOUNT: u64 = 100_000_000_000_000_000;
pub const STAKING_AMOUNT: u64 = 100_000_000_000_000_000;
pub const ECOSYSTEM_AMOUNT: u64 = 50_000_000_000_000_000;
pub const AIRDROP_AMOUNT: u64 = 50_000_000_000_000_000;
pub const TEAM_AMOUNT: u64 = 37_500_000_000_000_000;
pub const INVESTORS_AMOUNT: u64 = 37_500_000_000_000_000;
pub const TREASURY_AMOUNT: u64 = 25_000_000_000_000_000;

#[program]
pub mod kadeshx {
    use super::*;

    pub fn create_vesting(
        ctx: Context<CreateVesting>,
        total_amount: u64,
        start_time: i64,
        cliff_seconds: i64,
        duration_seconds: i64,
        tge_percentage: u64,
    ) -> Result<()> {
        // SECURITY FIX 1: Validate TGE percentage bounds (0-100%)
        require!(tge_percentage <= MAX_TGE_PERCENTAGE, VestingError::InvalidTgePercentage);
        
        // SECURITY FIX 2: Validate vesting parameters to prevent invalid configurations
        require!(total_amount > 0, VestingError::InvalidTotalAmount);
        require!(duration_seconds >= MIN_VESTING_DURATION, VestingError::InvalidDuration);
        require!(cliff_seconds >= 0, VestingError::InvalidCliff);
        
        // SECURITY FIX 3: Prevent overflow in time calculations
        let _cliff_end = start_time
            .checked_add(cliff_seconds)
            .ok_or(VestingError::MathOverflow)?;
        let _vesting_end = _cliff_end
            .checked_add(duration_seconds)
            .ok_or(VestingError::MathOverflow)?;
        
        // SECURITY FIX 8: Validate start_time is not in the past by more than reasonable drift
        // This prevents creating vesting schedules with retroactive start times
        let current_time = Clock::get()?.unix_timestamp;
        require!(
            start_time >= current_time.saturating_sub(300), // Allow 5 minutes drift
            VestingError::InvalidStartTime
        );

        let vesting = &mut ctx.accounts.vesting_account;
        vesting.owner = ctx.accounts.owner.key();
        vesting.mint = ctx.accounts.mint.key();
        vesting.total_amount = total_amount;
        vesting.released_amount = 0;
        vesting.start_time = start_time;
        vesting.cliff_seconds = cliff_seconds;
        vesting.duration_seconds = duration_seconds;
        vesting.tge_percentage = tge_percentage;
        vesting.bump = ctx.bumps.vesting_account;

        // TRANSFER (Token-2022 Uyumlu)
        let transfer_accounts = TransferChecked {
            from: ctx.accounts.payer_token_account.to_account_info(),
            to: ctx.accounts.vault_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
        };
        
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, transfer_accounts);

        token_interface::transfer_checked(cpi_ctx, total_amount, ctx.accounts.mint.decimals)?;

        msg!("Vesting oluşturuldu: {}", total_amount);
        Ok(())
    }

    pub fn claim_tokens(ctx: Context<ClaimTokens>) -> Result<()> {
        let vesting_account = &mut ctx.accounts.vesting_account;
        // SECURITY NOTE: unix_timestamp can be manipulated by validators within ~2 minutes.
        // For higher security requirements, consider using slot-based validation or
        // oracle-based time verification for institutional-grade deployments.
        let current_time = Clock::get()?.unix_timestamp;

        // SECURITY FIX 7: Handle Result from calculate_vested_amount
        let vested_amount = calculate_vested_amount(vesting_account, current_time)?;
        let claimable = vested_amount.saturating_sub(vesting_account.released_amount);

        require!(claimable > 0, VestingError::NoTokensToClaim);

        let seeds = &[
            b"vesting".as_ref(),
            vesting_account.owner.as_ref(),
            vesting_account.mint.as_ref(),
            &[vesting_account.bump],
        ];
        let signer = &[&seeds[..]];

        let transfer_accounts = TransferChecked {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.beneficiary_token_account.to_account_info(),
            authority: vesting_account.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
        };

        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, transfer_accounts, signer);

        token_interface::transfer_checked(cpi_ctx, claimable, ctx.accounts.mint.decimals)?;

        // SECURITY FIX 5: Use safe math with proper error handling instead of unwrap
        vesting_account.released_amount = vesting_account
            .released_amount
            .checked_add(claimable)
            .ok_or(VestingError::MathOverflow)?;
        Ok(())
    }
}

/// Calculates the vested amount with safe math operations
/// SECURITY FIX 6: All operations use checked arithmetic to prevent overflow/underflow
fn calculate_vested_amount(account: &VestingAccount, current_time: i64) -> Result<u64> {
    if current_time < account.start_time { return Ok(0); }
    
    // Calculate TGE amount with safe math
    let tge_amount = (account.total_amount as u128)
        .checked_mul(account.tge_percentage as u128)
        .ok_or(VestingError::MathOverflow)?
        .checked_div(MAX_TGE_PERCENTAGE as u128)
        .ok_or(VestingError::MathOverflow)? as u64;
    
    let remaining = account.total_amount.saturating_sub(tge_amount);
    
    // Calculate cliff end with safe math
    let cliff_end = account
        .start_time
        .checked_add(account.cliff_seconds)
        .ok_or(VestingError::MathOverflow)?;
    
    if current_time < cliff_end { return Ok(tge_amount); }
    
    // Calculate vesting end with safe math
    let vesting_end = cliff_end
        .checked_add(account.duration_seconds)
        .ok_or(VestingError::MathOverflow)?;
    
    if current_time >= vesting_end { return Ok(account.total_amount); }
    
    // Calculate linear vesting with safe math
    let time_passed = current_time
        .checked_sub(cliff_end)
        .ok_or(VestingError::MathOverflow)?;
    
    let vested_linear = (remaining as u128)
        .checked_mul(time_passed as u128)
        .ok_or(VestingError::MathOverflow)?
        .checked_div(account.duration_seconds as u128)
        .ok_or(VestingError::MathOverflow)? as u64;
    
    tge_amount
        .checked_add(vested_linear)
        .ok_or(VestingError::MathOverflow.into())
}

#[derive(Accounts)]
pub struct CreateVesting<'info> {
    #[account(mut)]
    pub payer: Signer<'info>, 

    #[account(
        mut, 
        constraint = payer_token_account.mint == mint.key(),
        constraint = payer_token_account.owner == payer.key()
    )]
    pub payer_token_account: InterfaceAccount<'info, TokenAccount>, 

    #[account(
        init,
        payer = payer,
        space = VestingAccount::LEN,
        seeds = [b"vesting", owner.key().as_ref(), mint.key().as_ref()],
        bump
    )]
    pub vesting_account: Account<'info, VestingAccount>,

    #[account(
        init,
        payer = payer,
        token::mint = mint,
        token::authority = vesting_account,
        seeds = [b"vault", vesting_account.key().as_ref()],
        bump,
        token::token_program = token_program
    )]
    pub vault_account: InterfaceAccount<'info, TokenAccount>,

    /// SECURITY FIX 4: Changed from UncheckedAccount to Signer
    /// This prevents arbitrary address assignment without signature verification
    pub owner: Signer<'info>,
    
    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
    
    // BURASI STANDART KALDI AMA 'INTERFACE' KULLANDIK
    pub token_program: Interface<'info, TokenInterface>, 
    
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ClaimTokens<'info> {
    #[account(mut)]
    pub beneficiary: Signer<'info>, 

    #[account(
        mut,
        seeds = [b"vesting", beneficiary.key().as_ref(), vesting_account.mint.as_ref()],
        bump = vesting_account.bump,
        constraint = vesting_account.owner == beneficiary.key()
    )]
    pub vesting_account: Account<'info, VestingAccount>,

    #[account(
        mut,
        seeds = [b"vault", vesting_account.key().as_ref()],
        bump
    )]
    pub vault_account: InterfaceAccount<'info, TokenAccount>, 

    #[account(
        mut,
        constraint = beneficiary_token_account.owner == beneficiary.key(),
        constraint = beneficiary_token_account.mint == vesting_account.mint
    )]
    pub beneficiary_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(address = vesting_account.mint)]
    pub mint: InterfaceAccount<'info, Mint>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[account]
pub struct VestingAccount {
    pub owner: Pubkey,
    pub mint: Pubkey,
    pub total_amount: u64,
    pub released_amount: u64,
    pub start_time: i64,
    pub cliff_seconds: i64,
    pub duration_seconds: i64,
    pub tge_percentage: u64,
    pub bump: u8,
}

impl VestingAccount {
    pub const LEN: usize = 8 + 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 1;
}

#[error_code]
pub enum VestingError {
    #[msg("Hatalı TGE yüzdesi.")]
    InvalidTgePercentage,
    #[msg("Çekilecek token yok.")]
    NoTokensToClaim,
    #[msg("Geçersiz toplam miktar.")]
    InvalidTotalAmount,
    #[msg("Geçersiz vade süresi.")]
    InvalidDuration,
    #[msg("Geçersiz cliff süresi.")]
    InvalidCliff,
    #[msg("Matematiksel işlem taşması.")]
    MathOverflow,
    #[msg("Geçersiz başlangıç zamanı.")]
    InvalidStartTime,
}