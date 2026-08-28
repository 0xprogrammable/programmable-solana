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

declare_id!("CfBnUaJwALVpd5Dtkt39zsvY9nwNTdrNxDvoxuCtKiR3");

/// Disposable V0 authority-kernel experiment. None of these instructions is a promised public ABI.
#[program]
pub mod programmable_core {
    use super::*;

    pub fn initialize_market_domain(
        ctx: Context<InitializeMarketDomainV0>,
        args: InitializeMarketDomainV0Args,
    ) -> Result<()> {
        crate::instructions::initialize_market_domain_v0::handle_initialize_market_domain_v0(
            ctx, args,
        )
    }

    pub fn deposit(ctx: Context<DepositV0>, args: DepositV0Args) -> Result<()> {
        crate::instructions::deposit_v0::handle_deposit_v0(ctx, args)
    }

    pub fn execute_engine_probe(
        ctx: Context<ExecuteEngineProbeV0>,
        args: ExecuteEngineProbeV0Args,
    ) -> Result<()> {
        crate::instructions::execute_engine_probe_v0::handle_execute_engine_probe_v0(ctx, args)
    }
}

#[cfg(test)]
mod tests {
    use anchor_lang::InstructionData;
    use engine_probe_interface::CORE_EXECUTE_ENGINE_PROBE_DISCRIMINATOR;

    use super::*;

    #[test]
    fn execute_discriminator_matches_engine_authentication_constant() {
        let data = crate::instruction::ExecuteEngineProbe {
            args: ExecuteEngineProbeV0Args {
                amount_in: 1,
                amount_out: 1,
                max_total_input_debit: 2,
                min_output_credit: 1,
                max_protocol_fee: 1,
                expires_at_slot: 1,
            },
        }
        .data();

        assert_eq!(
            &data[..CORE_EXECUTE_ENGINE_PROBE_DISCRIMINATOR.len()],
            &CORE_EXECUTE_ENGINE_PROBE_DISCRIMINATOR
        );
    }
}
