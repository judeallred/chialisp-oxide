//! Original reclaimable-payment teaching policy: template execution is separate
//! from committed coin instances and actual SDK transaction attempts.
mod ledger;

use super::curried;
use anyhow::{Result, ensure};
use serde::Serialize;

pub const PROTOCOL: &str = "chialisp-oxide-boomerang-instances/v1";
const SCOPE: &str = "Original teaching policy with one publicly known demonstration key and fixed payment terms. Each of five compiled templates has its own independently constructed, SDK-cross-checked curried instance. Primary VM observations, SDK consensus attempts and actual source mutants are separate finite evidence; no automatic refund service, mainnet/fork universality, exhaustive authorization, BLS proof, optional-output proof, compiler preservation, authenticated execution or production-readiness claim.";

#[derive(Serialize)]
pub struct CoinInstances {
    pub protocol: &'static str,
    pub role: &'static str,
    pub scheme: &'static str,
    pub examples: Vec<curried::Example>,
    pub ledger: Vec<ledger::Comparison>,
    pub scope: &'static str,
}

pub(super) fn validate(
    specs: &[curried::Spec],
    programs: &[(&str, bool, Vec<u8>)],
    max_cost: u64,
) -> Result<CoinInstances> {
    ensure!(
        specs.len() == 1,
        "boomerang requires its one declared demonstration instance"
    );
    ensure!(
        specs[0].label == "public_demo_payment" && specs[0].page_result.is_none(),
        "boomerang instance is an original teaching contract, not a published canonical program"
    );
    ensure!(
        programs
            .iter()
            .map(|(backend, mode, _)| (*backend, *mode))
            .collect::<Vec<_>>()
            == [
                ("rust_mir", false),
                ("rust_mir", true),
                ("chialisp", true),
                ("rue_source", false),
                ("rue_source", true)
            ],
        "boomerang instances require all five compiler builds"
    );
    let examples = curried::validate("boomerang", specs, programs, max_cost)?;
    let ledger = ledger::validate(programs, &examples)?;
    Ok(CoinInstances {
        protocol: PROTOCOL,
        role: "original_teaching_coin_instances",
        scheme: curried::SCHEME,
        examples,
        ledger,
        scope: SCOPE,
    })
}
