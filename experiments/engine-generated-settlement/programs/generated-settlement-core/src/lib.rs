pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod math;
pub mod state;
pub mod validation;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("EJKx7XFp6CZQuAHD6AC14g7nUKeczJMr2TX9XRUEjs36");

/// Disposable engine-generated settlement experiment. This is not a public ABI.
#[program]
pub mod programmable_generated_settlement_core {
    use super::*;

    pub fn initialize_market_domain(
        ctx: Context<InitializeMarketDomainV0>,
        args: InitializeMarketDomainV0Args,
    ) -> Result<()> {
        instructions::initialize_market_domain_v0::handle_initialize_market_domain_v0(ctx, args)
    }

    pub fn deposit(ctx: Context<DepositV0>, args: DepositV0Args) -> Result<()> {
        instructions::deposit_v0::handle_deposit_v0(ctx, args)
    }

    pub fn execute_engine_generated_probe<'info>(
        ctx: Context<'info, ExecuteEngineGeneratedProbeV0<'info>>,
        args: ExecuteEngineGeneratedProbeV0Args,
    ) -> Result<()> {
        instructions::execute_engine_generated_probe_v0::handle_execute_engine_generated_probe_v0(
            ctx, args,
        )
    }
}
