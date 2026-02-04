# Task: Create Production-Grade Test Suite for KadeshX Vesting Contract

## Context
This is a Solana Anchor vesting contract with Token-2022 support. We need comprehensive tests to win a security bounty competition against another researcher.

## Current State
- Contract has security fixes applied (safe math, validations, etc.)
- Competitor has comprehensive security fixes
- Our advantage: Testing + CI/CD + Gas Optimization

## Deliverables

### 1. Comprehensive Test Suite (tests/kadeshx.ts)
Create tests covering:
- Happy path: create vesting → wait → claim
- Edge cases: Zero amounts, max u64 values, boundary timestamps
- Security tests: Unauthorized access, overflow attempts
- Mathematical verification: TGE calculations, linear vesting math
- Fuzzing-style tests: Random valid inputs

### 2. Gas Optimization Report (GAS_REPORT.md)
Analyze and document:
- Current CU usage per instruction
- Optimization recommendations
- Compute unit benchmarks

### 3. GitHub Actions CI/CD (.github/workflows/anchor.yml)
Complete CI with:
- Anchor build
- Rust tests
- TypeScript tests
- Security linting

## Requirements
- All tests must pass
- Use Anchor Bankrun for fast testing (if available) or standard anchor test
- Include CU measurement in tests
- Document expected vs actual gas costs

When finished, commit with message: "feat: production test suite + gas optimization report"
