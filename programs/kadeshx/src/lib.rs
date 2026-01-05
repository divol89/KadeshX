use anchor_lang::prelude::*;

declare_id!("3Wnf265xSEsFg1i6FAEGKqi8QoJedsh3dzqvZGZriLGA");

#[program]
pub mod kadeshx {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
