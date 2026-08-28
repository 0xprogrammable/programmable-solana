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

declare_id!("Bwhiw9S9ZdHkEhFF2Ps89HMxa5dHX1xSbdsGZ8W3qR2b");

/// Disposable routed callback authentication experiment. This is not a public ABI.
#[program]
pub mod programmable_routed_callback_core {
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

    pub fn authorize_spend_v0(
        ctx: Context<AuthorizeSpendV0>,
        args: AuthorizeSpendV0Args,
    ) -> Result<()> {
        instructions::authorize_spend_v0::handle_authorize_spend_v0(ctx, args)
    }

    pub fn execute_callback_authenticated_probe_v0<'info>(
        ctx: Context<'info, ExecuteCallbackAuthenticatedProbeV0<'info>>,
        args: ExecuteCallbackAuthenticatedProbeV0Args,
    ) -> Result<()> {
        instructions::execute_callback_authenticated_probe_v0::handle_execute_callback_authenticated_probe_v0(
            ctx, args,
        )
    }
}
