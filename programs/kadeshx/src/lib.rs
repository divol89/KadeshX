use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

declare_id!("EsuD4bWYtzJmHJpzjqo9xXPDCJwoNzrK5BsGPw83yiE5");

pub const DECIMALS: u32 = 9;
pub const SECONDS_PER_MONTH: i64 = 30 * 24 * 60 * 60;

// Token distribution constants (preserved)
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

    /// Initializes a new vesting schedule for a beneficiary.
    /// 
    /// # Arguments
    /// * `total_amount` - Total tokens to be vested
    /// * `start_time` - Unix timestamp when vesting starts
    /// * `cliff_seconds` - Duration of cliff period in seconds
    /// * `duration_seconds` - Total vesting duration after cliff (must be > 0)
    /// * `tge_percentage_bps` - TGE release in Basis Points (10000 = 100%)
    pub fn create_vesting(
        ctx: Context<CreateVesting>,
        total_amount: u64,
        start_time: i64,
        cliff_seconds: i64,
        duration_seconds: i64,
        tge_percentage_bps: u64,
    ) -> Result<()> {
        // Validate inputs
        require!(total_amount > 0, VestingError::InvalidAmount);
        require!(tge_percentage_bps <= 10000, VestingError::InvalidTgePercentage);
        require!(duration_seconds > 0, VestingError::InvalidDuration);
        require!(cliff_seconds >= 0, VestingError::InvalidCliff);
        
        // Validate total vesting period doesn't overflow
        start_time
            .checked_add(cliff_seconds)
            .and_then(|v| v.checked_add(duration_seconds))
            .ok_or(VestingError::MathOverflow)?;

        let vesting = &mut ctx.accounts.vesting_account;
        vesting.owner = ctx.accounts.owner.key();
        vesting.mint = ctx.accounts.mint.key();
        vesting.total_amount = total_amount;
        vesting.released_amount = 0;
        vesting.start_time = start_time;
        vesting.cliff_seconds = cliff_seconds;
        vesting.duration_seconds = duration_seconds;
        vesting.tge_percentage = tge_percentage_bps;
        vesting.bump = ctx.bumps.vesting_account;

        // Transfer tokens to the program vault
        let transfer_accounts = TransferChecked {
            from: ctx.accounts.payer_token_account.to_account_info(),
            to: ctx.accounts.vault_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
        };
        
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), transfer_accounts);
        token_interface::transfer_checked(cpi_ctx, total_amount, ctx.accounts.mint.decimals)?;

        emit!(VestingCreated {
            beneficiary: vesting.owner,
            total_amount,
            start_time,
            cliff_seconds,
            duration_seconds,
            tge_percentage_bps,
        });

        Ok(())
    }

    /// Claims currently vested and available tokens.
    pub fn claim_tokens(ctx: Context<ClaimTokens>) -> Result<()> {
        let vesting_account = &mut ctx.accounts.vesting_account;
        let current_time = Clock::get()?.unix_timestamp;

        let vested_amount = calculate_vested_amount(vesting_account, current_time)?;
        let claimable = vested_amount
            .saturating_sub(vesting_account.released_amount);

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

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(), 
            transfer_accounts, 
            signer
        );

        token_interface::transfer_checked(cpi_ctx, claimable, ctx.accounts.mint.decimals)?;

        vesting_account.released_amount = vesting_account.released_amount
            .checked_add(claimable)
            .ok_or(VestingError::MathOverflow)?;

        emit!(TokensClaimed {
            beneficiary: vesting_account.owner,
            amount: claimable,
            timestamp: current_time,
            total_released: vesting_account.released_amount,
        });

        Ok(())
    }
}

/// Precise linear vesting calculation with TGE release.
/// 
/// # Returns
/// * `Ok(u64)` - Total vested amount at current time
/// * `Err(VestingError)` - If math overflow occurs
fn calculate_vested_amount(account: &VestingAccount, current_time: i64) -> Result<u64> {
    if current_time < account.start_time {
        return Ok(0);
    }

    // TGE Amount = Total * TGE_BPS / 10000
    let tge_amount = (account.total_amount as u128)
        .checked_mul(account.tge_percentage as u128)
        .ok_or(VestingError::MathOverflow)?
        .checked_div(10000)
        .ok_or(VestingError::MathOverflow)? as u64;

    let remaining_to_vest = account.total_amount.saturating_sub(tge_amount);
    
    let cliff_end = account.start_time
        .checked_add(account.cliff_seconds)
        .ok_or(VestingError::MathOverflow)?;

    if current_time < cliff_end {
        return Ok(tge_amount);
    }

    let vesting_end = cliff_end
        .checked_add(account.duration_seconds)
        .ok_or(VestingError::MathOverflow)?;
        
    if current_time >= vesting_end {
        return Ok(account.total_amount);
    }

    // Linear portion = Remaining * (Time Passed) / (Total Duration)
    let time_passed = current_time
        .checked_sub(cliff_end)
        .ok_or(VestingError::MathOverflow)?;
        
    let vested_linear = (remaining_to_vest as u128)
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

    /// CHECK: Safe - only used as seed for PDA
    pub owner: UncheckedAccount<'info>,
    
    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
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
    
    /// Returns remaining claimable amount
    pub fn remaining_amount(&self) -> u64 {
        self.total_amount.saturating_sub(self.released_amount)
    }
    
    /// Checks if vesting is fully complete
    pub fn is_fully_vested(&self, current_time: i64) -> bool {
        let vesting_end = self.start_time.saturating_add(self.cliff_seconds).saturating_add(self.duration_seconds);
        current_time >= vesting_end
    }
}

#[event]
pub struct VestingCreated {
    pub beneficiary: Pubkey,
    pub total_amount: u64,
    pub start_time: i64,
    pub cliff_seconds: i64,
    pub duration_seconds: i64,
    pub tge_percentage_bps: u64,
}

#[event]
pub struct TokensClaimed {
    pub beneficiary: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
    pub total_released: u64,
}

#[error_code]
pub enum VestingError {
    #[msg("Invalid TGE percentage. Must be 0-10000 BPS")]
    InvalidTgePercentage,
    #[msg("No tokens available to claim")]
    NoTokensToClaim,
    #[msg("Math overflow in calculation")]
    MathOverflow,
    #[msg("Invalid amount. Must be greater than 0")]
    InvalidAmount,
    #[msg("Invalid duration. Must be greater than 0")]
    InvalidDuration,
    #[msg("Invalid cliff. Must be 0 or greater")]
    InvalidCliff,
}
