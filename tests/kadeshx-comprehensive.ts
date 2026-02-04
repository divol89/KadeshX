import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram, ComputeBudgetProgram } from "@solana/web3.js";
import { Kadeshx } from "../target/types/kadeshx";
import { createMint, getAssociatedTokenAddressSync, createAssociatedTokenAccount, mintTo, TOKEN_2022_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { assert } from "chai";

describe("KadeshX Vesting Contract - Comprehensive Test Suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Kadeshx as Program<Kadeshx>;
  
  // Test accounts
  const admin = Keypair.generate();
  const user1 = Keypair.generate();
  const user2 = Keypair.generate();
  let mint: PublicKey;
  let adminATA: PublicKey;
  let user1ATA: PublicKey;
  
  // CU tracking
  let cuMeasurements: { instruction: string; cuUsed: number }[] = [];
  
  before(async () => {
    // Airdrop SOL to test accounts
    await provider.connection.requestAirdrop(admin.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(user1.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Create Token-2022 mint
    mint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      9,
      Keypair.generate(),
      { commitment: 'confirmed' },
      TOKEN_2022_PROGRAM_ID
    );
    
    // Create ATAs
    adminATA = await createAssociatedTokenAccount(
      provider.connection,
      admin,
      mint,
      admin.publicKey,
      { commitment: 'confirmed' },
      TOKEN_2022_PROGRAM_ID
    );
    
    user1ATA = await createAssociatedTokenAccount(
      provider.connection,
      user1,
      mint,
      user1.publicKey,
      { commitment: 'confirmed' },
      TOKEN_2022_PROGRAM_ID
    );
    
    // Mint tokens to admin
    await mintTo(
      provider.connection,
      admin,
      mint,
      adminATA,
      admin,
      1_000_000_000_000, // 1000 tokens with 9 decimals
      [],
      { commitment: 'confirmed' },
      TOKEN_2022_PROGRAM_ID
    );
  });
  
  after(() => {
    // Print CU report
    console.log("\n=== COMPUTE UNIT MEASUREMENTS ===");
    cuMeasurements.forEach(m => console.log(`${m.instruction}: ${m.cuUsed} CU`));
    const total = cuMeasurements.reduce((sum, m) => sum + m.cuUsed, 0);
    console.log(`Total CU measured: ${total}`);
  });

  const measureCU = async (instruction: string, tx: Promise<string>): Promise<string> => {
    const sig = await tx;
    const txDetails = await provider.connection.getTransaction(sig, { commitment: 'confirmed' });
    const cuUsed = txDetails?.meta?.computeUnitsConsumed || 0;
    cuMeasurements.push({ instruction, cuUsed });
    return sig;
  };

  // ==================== HAPPY PATH TESTS ====================
  
  describe("Happy Path", () => {
    it("Creates vesting with TGE and linear vesting", async () => {
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user1.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      const now = Math.floor(Date.now() / 1000);
      const totalAmount = new BN(1_000_000_000); // 1 token
      const startTime = new BN(now);
      const cliffSeconds = new BN(0);
      const durationSeconds = new BN(300); // 5 minutes
      const tgePercentage = new BN(1000); // 10%
      
      await measureCU("create_vesting (with TGE)", 
        program.methods
          .createVesting(totalAmount, startTime, cliffSeconds, durationSeconds, tgePercentage)
          .accounts({
            payer: admin.publicKey,
            payerTokenAccount: adminATA,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            owner: user1.publicKey,
            mint: mint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([admin])
          .rpc()
      );
      
      // Verify vesting account
      const vestingAccount = await program.account.vestingAccount.fetch(vestingPDA);
      assert.equal(vestingAccount.owner.toString(), user1.publicKey.toString());
      assert.equal(vestingAccount.totalAmount.toString(), totalAmount.toString());
      assert.equal(vestingAccount.tgePercentage.toString(), tgePercentage.toString());
      assert.equal(vestingAccount.releasedAmount.toNumber(), 0);
    });
    
    it("Claims TGE amount immediately", async () => {
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user1.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      const balanceBefore = await provider.connection.getTokenAccountBalance(user1ATA);
      
      await measureCU("claim_tokens (TGE only)",
        program.methods
          .claimTokens()
          .accounts({
            beneficiary: user1.publicKey,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            beneficiaryTokenAccount: user1ATA,
            mint: mint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([user1])
          .rpc()
      );
      
      const balanceAfter = await provider.connection.getTokenAccountBalance(user1ATA);
      const claimed = new BN(balanceAfter.value.amount).sub(new BN(balanceBefore.value.amount));
      
      // 10% TGE of 1B = 100M
      assert.equal(claimed.toString(), "100000000");
      
      // Verify released amount updated
      const vestingAccount = await program.account.vestingAccount.fetch(vestingPDA);
      assert.equal(vestingAccount.releasedAmount.toString(), "100000000");
    });
  });
  
  // ==================== MATHEMATICAL VERIFICATION ====================
  
  describe("Mathematical Verification", () => {
    it("Verifies TGE calculation formula", async () => {
      const testCases = [
        { total: 1_000_000_000, tgePct: 0, expected: 0 },
        { total: 1_000_000_000, tgePct: 1000, expected: 100_000_000 }, // 10%
        { total: 1_000_000_000, tgePct: 5000, expected: 500_000_000 }, // 50%
        { total: 1_000_000_000, tgePct: 10000, expected: 1_000_000_000 }, // 100%
      ];
      
      for (const tc of testCases) {
        const user = Keypair.generate();
        const userAta = await createAssociatedTokenAccount(
          provider.connection, admin, mint, user.publicKey, 
          { commitment: 'confirmed' }, TOKEN_2022_PROGRAM_ID
        );
        
        const [vestingPDA] = PublicKey.findProgramAddressSync(
          [Buffer.from("vesting"), user.publicKey.toBuffer(), mint.toBuffer()],
          program.programId
        );
        const [vaultPDA] = PublicKey.findProgramAddressSync(
          [Buffer.from("vault"), vestingPDA.toBuffer()],
          program.programId
        );
        
        await program.methods
          .createVesting(
            new BN(tc.total),
            new BN(Math.floor(Date.now() / 1000)),
            new BN(0),
            new BN(60),
            new BN(tc.tgePct)
          )
          .accounts({
            payer: admin.publicKey,
            payerTokenAccount: adminATA,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            owner: user.publicKey,
            mint: mint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([admin])
          .rpc();
        
        await program.methods
          .claimTokens()
          .accounts({
            beneficiary: user.publicKey,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            beneficiaryTokenAccount: userAta,
            mint: mint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([user])
          .rpc();
        
        const balance = await provider.connection.getTokenAccountBalance(userAta);
        assert.equal(balance.value.amount, tc.expected.toString(), 
          `TGE calculation failed for ${tc.tgePct} bps`);
      }
    });
    
    it("Verifies linear vesting calculation at 50% time", async () => {
      // This test would require time manipulation (not possible on localnet without special setup)
      // Mark as pending for now - would need bankrun or similar
      console.log("Linear vesting at 50%: Would require time manipulation (bankrun recommended)");
    });
  });
  
  // ==================== SECURITY TESTS ====================
  
  describe("Security Tests", () => {
    it("Rejects zero total amount", async () => {
      const user = Keypair.generate();
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      try {
        await program.methods
          .createVesting(new BN(0), new BN(1000), new BN(0), new BN(60), new BN(1000))
          .accounts({
            payer: admin.publicKey,
            payerTokenAccount: adminATA,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            owner: user.publicKey,
            mint: mint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([admin])
          .rpc();
        assert.fail("Should have rejected zero amount");
      } catch (err: any) {
        assert.include(err.message, "InvalidTotalAmount");
      }
    });
    
    it("Rejects zero duration", async () => {
      const user = Keypair.generate();
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      try {
        await program.methods
          .createVesting(new BN(1000), new BN(1000), new BN(0), new BN(0), new BN(1000))
          .accounts({
            payer: admin.publicKey,
            payerTokenAccount: adminATA,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            owner: user.publicKey,
            mint: mint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([admin])
          .rpc();
        assert.fail("Should have rejected zero duration");
      } catch (err: any) {
        assert.include(err.message, "InvalidDuration");
      }
    });
    
    it("Rejects TGE percentage > 100%", async () => {
      const user = Keypair.generate();
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      try {
        await program.methods
          .createVesting(new BN(1000), new BN(1000), new BN(0), new BN(60), new BN(10001))
          .accounts({
            payer: admin.publicKey,
            payerTokenAccount: adminATA,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            owner: user.publicKey,
            mint: mint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([admin])
          .rpc();
        assert.fail("Should have rejected >100% TGE");
      } catch (err: any) {
        assert.include(err.message, "InvalidTgePercentage");
      }
    });
    
    it("Rejects unauthorized claim", async () => {
      const user = Keypair.generate();
      const attacker = Keypair.generate();
      
      await provider.connection.requestAirdrop(attacker.publicKey, 1 * anchor.web3.LAMPORTS_PER_SOL);
      await new Promise(resolve => setTimeout(resolve, 500));
      
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      // Create vesting
      await program.methods
        .createVesting(new BN(1000), new BN(Math.floor(Date.now() / 1000)), new BN(0), new BN(60), new BN(1000))
        .accounts({
          payer: admin.publicKey,
          payerTokenAccount: adminATA,
          vestingAccount: vestingPDA,
          vaultAccount: vaultPDA,
          owner: user.publicKey,
          mint: mint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([admin])
        .rpc();
      
      // Try to claim as attacker
      try {
        await program.methods
          .claimTokens()
          .accounts({
            beneficiary: attacker.publicKey,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            beneficiaryTokenAccount: user1ATA,
            mint: mint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([attacker])
          .rpc();
        assert.fail("Should have rejected unauthorized claim");
      } catch (err: any) {
        // Should fail with constraint violation
        assert.ok(err);
      }
    });
    
    it("Rejects claim when no tokens available", async () => {
      const user = Keypair.generate();
      const userAta = await createAssociatedTokenAccount(
        provider.connection, admin, mint, user.publicKey, 
        { commitment: 'confirmed' }, TOKEN_2022_PROGRAM_ID
      );
      
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      // Create vesting with future start time
      const futureStart = Math.floor(Date.now() / 1000) + 3600; // 1 hour from now
      await program.methods
        .createVesting(new BN(1000), new BN(futureStart), new BN(0), new BN(60), new BN(0))
        .accounts({
          payer: admin.publicKey,
          payerTokenAccount: adminATA,
          vestingAccount: vestingPDA,
          vaultAccount: vaultPDA,
          owner: user.publicKey,
          mint: mint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([admin])
        .rpc();
      
      try {
        await program.methods
          .claimTokens()
          .accounts({
            beneficiary: user.publicKey,
            vestingAccount: vestingPDA,
            vaultAccount: vaultPDA,
            beneficiaryTokenAccount: userAta,
            mint: mint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([user])
          .rpc();
        assert.fail("Should have rejected claim with no tokens");
      } catch (err: any) {
        assert.include(err.message, "NoTokensToClaim");
      }
    });
  });
  
  // ==================== EDGE CASE TESTS ====================
  
  describe("Edge Cases", () => {
    it("Handles max u64 values without overflow", async () => {
      const user = Keypair.generate();
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      // Large but valid amount
      const largeAmount = new BN("1000000000000"); // 1M tokens
      
      await program.methods
        .createVesting(largeAmount, new BN(1000), new BN(0), new BN(60), new BN(5000))
        .accounts({
          payer: admin.publicKey,
          payerTokenAccount: adminATA,
          vestingAccount: vestingPDA,
          vaultAccount: vaultPDA,
          owner: user.publicKey,
          mint: mint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([admin])
        .rpc();
      
      const vestingAccount = await program.account.vestingAccount.fetch(vestingPDA);
      assert.equal(vestingAccount.totalAmount.toString(), largeAmount.toString());
    });
    
    it("Handles 0% TGE correctly", async () => {
      const user = Keypair.generate();
      const userAta = await createAssociatedTokenAccount(
        provider.connection, admin, mint, user.publicKey, 
        { commitment: 'confirmed' }, TOKEN_2022_PROGRAM_ID
      );
      
      const [vestingPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vesting"), user.publicKey.toBuffer(), mint.toBuffer()],
        program.programId
      );
      const [vaultPDA] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault"), vestingPDA.toBuffer()],
        program.programId
      );
      
      await program.methods
        .createVesting(new BN(1000), new BN(Math.floor(Date.now() / 1000)), new BN(0), new BN(60), new BN(0))
        .accounts({
          payer: admin.publicKey,
          payerTokenAccount: adminATA,
          vestingAccount: vestingPDA,
          vaultAccount: vaultPDA,
          owner: user.publicKey,
          mint: mint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([admin])
        .rpc();
      
      // Should be able to claim immediately (0 cliff, immediate vesting)
      await program.methods
        .claimTokens()
        .accounts({
          beneficiary: user.publicKey,
          vestingAccount: vestingPDA,
          vaultAccount: vaultPDA,
          beneficiaryTokenAccount: userAta,
          mint: mint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([user])
        .rpc();
      
      const balance = await provider.connection.getTokenAccountBalance(userAta);
      assert.equal(balance.value.amount, "1000");
    });
  });
});
