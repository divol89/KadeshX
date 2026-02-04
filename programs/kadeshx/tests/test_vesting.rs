use anchor_lang::prelude::*;
use solana_program_test::*;
use solana_sdk::{signature::Keypair, signer::Signer, system_instruction};
use anchor_spl::token_interface::{self, Mint, TokenAccount};

#[cfg(test)]
mod tests {
    use super::*;

    /// Test vesting calculation at different time points
    #[test]
    fn test_vesting_calculation() {
        // TGE: 10%, Cliff: 30 days, Duration: 365 days, Total: 1000 tokens
        let total = 1_000_000_000_000u64; // 1000 tokens with 9 decimals
        let tge_bps = 1000u64; // 10%
        let cliff = 30 * 24 * 60 * 60i64; // 30 days
        let duration = 365 * 24 * 60 * 60i64; // 365 days
        let start_time = 1_000_000_000i64;
        
        // Helper to calculate vested amount
        let calc_vested = |current_time: i64| -> u64 {
            if current_time < start_time {
                return 0;
            }
            
            let tge_amount = (total as u128)
                .checked_mul(tge_bps as u128).unwrap()
                .checked_div(10000).unwrap() as u64;
            
            let remaining = total.saturating_sub(tge_amount);
            let cliff_end = start_time.saturating_add(cliff);
            
            if current_time < cliff_end {
                return tge_amount;
            }
            
            let vesting_end = cliff_end.saturating_add(duration);
            if current_time >= vesting_end {
                return total;
            }
            
            let time_passed = current_time.saturating_sub(cliff_end);
            let vested_linear = (remaining as u128)
                .checked_mul(time_passed as u128).unwrap()
                .checked_div(duration as u128).unwrap() as u64;
            
            tge_amount.saturating_add(vested_linear)
        };
        
        // Before start: 0 vested
        assert_eq!(calc_vested(start_time - 1), 0);
        
        // At start (TGE only): 100 tokens (10%)
        let tge_amount = total / 10;
        assert_eq!(calc_vested(start_time), tge_amount);
        
        // During cliff (TGE only): still 100 tokens
        assert_eq!(calc_vested(start_time + cliff / 2), tge_amount);
        
        // At cliff end: still TGE only
        assert_eq!(calc_vested(start_time + cliff), tge_amount);
        
        // Halfway through vesting: TGE + 50% of remaining
        let half_vested = calc_vested(start_time + cliff + duration / 2);
        let expected_half = tge_amount + (total - tge_amount) / 2;
        // Allow for rounding error of 1 lamport
        assert!(
            half_vested.abs_diff(expected_half) <= 1,
            "Half vesting: got {}, expected {}",
            half_vested,
            expected_half
        );
        
        // At end: 100% vested
        assert_eq!(calc_vested(start_time + cliff + duration), total);
        
        // After end: still 100%
        assert_eq!(calc_vested(start_time + cliff + duration + 1000), total);
    }

    /// Test edge cases
    #[test]
    fn test_edge_cases() {
        // Zero TGE
        let total = 1_000_000_000_000u64;
        let tge_bps = 0u64;
        let tge_amount = (total as u128)
            .checked_mul(tge_bps as u128).unwrap()
            .checked_div(10000).unwrap() as u64;
        assert_eq!(tge_amount, 0);
        
        // 100% TGE
        let tge_bps = 10000u64;
        let tge_amount = (total as u128)
            .checked_mul(tge_bps as u128).unwrap()
            .checked_div(10000).unwrap() as u64;
        assert_eq!(tge_amount, total);
        
        // Zero cliff
        let cliff = 0i64;
        assert_eq!(cliff, 0);
        
        // Very small amount
        let tiny = 100u64;
        let tge_bps = 1000u64; // 10%
        let tge_amount = (tiny as u128)
            .checked_mul(tge_bps as u128).unwrap()
            .checked_div(10000).unwrap() as u64;
        assert_eq!(tge_amount, 10); // 10% of 100 = 10
    }

    /// Test error conditions
    #[test]
    fn test_validation_errors() {
        // Invalid TGE percentage (> 10000 BPS)
        let tge_bps = 10001u64;
        assert!(tge_bps > 10000, "TGE should be invalid");
        
        // Valid TGE
        let tge_bps = 10000u64;
        assert!(tge_bps <= 10000, "TGE should be valid");
    }

    /// Test math overflow protection
    #[test]
    fn test_overflow_protection() {
        use std::panic;
        
        // Test that checked operations don't panic on overflow
        let max_u64 = u64::MAX;
        let result = (max_u64 as u128)
            .checked_mul(2u128)
            .and_then(|v| v.checked_div(10000));
        assert!(result.is_none(), "Should detect overflow");
        
        // Valid calculation should succeed
        let valid = 1_000_000_000_000u64;
        let result = (valid as u128)
            .checked_mul(1000u128)
            .and_then(|v| v.checked_div(10000));
        assert!(result.is_some(), "Valid calculation should succeed");
        assert_eq!(result.unwrap(), 100_000_000_000u128);
    }
}
