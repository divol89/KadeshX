import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { Kadeshx } from "../target/types/kadeshx";

describe("KadeshX Devnet Live Test", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Kadeshx as Program<Kadeshx>;

  // 1. MANUEL SABİTLER (Import hatasını çözer)
  // Kütüphaneden çekmek yerine adresi elle yazdık.
  const TOKEN_2022_PROGRAM_ID = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
  
  const KDX_MINT = new PublicKey("4SLKY8288pGFjHBExYg93sN4dH6F8Fib8W2Q4majDkWA");
  const MY_ATA = new PublicKey("TKAr9unobugMmdk2PKZRZ62MyBb31DC9FXirXQEoeHk");

  it("Devnet üzerinde Gerçek Token ile Vesting Oluşturuyor", async () => {
    const admin = provider.wallet.publicKey;

    // PDA Hesapla
    const [vestingAccountPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vesting"), admin.toBuffer(), KDX_MINT.toBuffer()],
      program.programId
    );
    const [vaultAccountPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), vestingAccountPda.toBuffer()],
      program.programId
    );

    console.log("Vesting PDA:", vestingAccountPda.toString());

    try {
      const tx = await program.methods
        .createVesting(
          new anchor.BN("1000000000"), 
          new anchor.BN(Math.floor(Date.now() / 1000)),
          new anchor.BN(0),
          new anchor.BN(300), 
          new anchor.BN(1000) 
        )
        // 'as any' ekleyerek VS Code'un tip hatalarını (Kırmızı Çizgi) susturuyoruz
        .accounts({
          vestingAccount: vestingAccountPda,
          vaultAccount: vaultAccountPda,
          mint: KDX_MINT,
          payer: admin,
          payerTokenAccount: MY_ATA, 
          owner: admin,
          systemProgram: anchor.web3.SystemProgram.programId,
          
          // Elle tanımladığımız adresi buraya veriyoruz
          tokenProgram: TOKEN_2022_PROGRAM_ID, 
          
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        } as any) 
        .rpc();

      console.log("✅ BAŞARILI! Tx:", tx);
      console.log("Link: https://explorer.solana.com/tx/" + tx + "?cluster=devnet");
    } catch (err) {
      console.error("❌ HATA:", err);
    }
  });
});