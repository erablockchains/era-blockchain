//! Public genesis bindings and deterministic category accounting (native tooling only).
use crate::*;
use alloc::{vec, vec::Vec};
use era_v14_custody_governance::CustodyCategory;
use sp_core::{ed25519, sr25519, Pair};
use std::collections::BTreeMap;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub account: AccountId,
    pub babe: sr25519::Public,
    pub grandpa: ed25519::Public,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inputs {
    pub development: bool,
    #[serde(default)]
    pub production_bindings_confirmed: bool,
    #[serde(default)]
    pub operating_allocation_approved: bool,
    #[serde(default)]
    pub fresh_consensus_keys_confirmed: bool,
    pub founders: [AccountId; 3],
    pub beneficiaries: [AccountId; 5],
    pub sudo: AccountId,
    pub community: AccountId,
    pub authorities: [Authority; 4],
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Allocation {
    pub category: &'static str,
    pub purpose: &'static str,
    pub account: AccountId,
    /// Decimal base-unit string prevents JSON number precision loss.
    pub base_units: String,
}
impl Inputs {
    pub fn synthetic() -> Self {
        fn account(seed: &str) -> AccountId {
            sr25519::Pair::from_string(seed, None)
                .unwrap()
                .public()
                .into()
        }
        let mut founders = [
            account("//RelaunchFounder1"),
            account("//RelaunchFounder2"),
            account("//RelaunchFounder3"),
        ];
        founders.sort();
        Self {
            development: true,
            production_bindings_confirmed: false,
            operating_allocation_approved: false,
            fresh_consensus_keys_confirmed: false,
            founders,
            beneficiaries: core::array::from_fn(|i| account(&format!("//RelaunchVesting{i}"))),
            sudo: account("//RelaunchSudo"),
            community: account("//RelaunchCommunity"),
            authorities: ["Alice", "Bob", "Charlie", "Dave"].map(|seed| {
                let uri = format!("//{seed}");
                Authority {
                    account: account(&uri),
                    babe: sr25519::Pair::from_string(&uri, None).unwrap().public(),
                    grandpa: ed25519::Pair::from_string(&uri, None).unwrap().public(),
                }
            }),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        let reject = |why: &str| Err(why.to_owned());
        if !(self.founders[0] < self.founders[1] && self.founders[1] < self.founders[2]) {
            return reject("founders must be sorted and distinct");
        }
        let mut roles = Vec::new();
        roles.extend(self.founders.clone());
        roles.extend(self.beneficiaries.clone());
        roles.push(self.sudo.clone());
        roles.push(self.community.clone());
        roles.extend(self.authorities.iter().map(|v| v.account.clone()));
        roles.extend(FounderCustody::custody_accounts());
        roles.extend([
            SecurityBudget::staking_pot_account(),
            SecurityBudget::treasury_account(),
            SecurityBudget::fee_collection_account(),
            RewardReserve::reward_pot_account(),
        ]);
        for (i, account) in roles.iter().enumerate() {
            if *account == AccountId::new([0; 32]) || roles[..i].contains(account) {
                return reject("zero or overlapping genesis account roles");
            }
        }
        for (i, v) in self.authorities.iter().enumerate() {
            if v.babe == sr25519::Public::from_raw([0; 32])
                || v.grandpa == ed25519::Public::from_raw([0; 32])
            {
                return reject("zero authority key");
            }
            if self.authorities[..i]
                .iter()
                .any(|x| x.babe == v.babe || x.grandpa == v.grandpa)
            {
                return reject("duplicate consensus key");
            }
        }
        if !self.development {
            if !(self.production_bindings_confirmed
                && self.operating_allocation_approved
                && self.fresh_consensus_keys_confirmed)
            {
                return reject("production bindings, operating allocation and fresh consensus keys require explicit owner confirmation");
            }
            let fixture = Self::synthetic();
            let mut forbidden = vec![fixture.sudo, fixture.community];
            forbidden.extend(fixture.founders);
            forbidden.extend(fixture.beneficiaries);
            for seed in ["Alice", "Bob", "Charlie", "Dave", "Eve", "Ferdie"] {
                let uri = format!("//{seed}");
                let sr = sr25519::Pair::from_string(&uri, None).unwrap().public();
                let ed = ed25519::Pair::from_string(&uri, None).unwrap().public();
                forbidden.push(sr.into());
                if self
                    .authorities
                    .iter()
                    .any(|v| v.babe == sr || v.grandpa == ed)
                {
                    return reject("development consensus key in production inputs");
                }
            }
            if roles.iter().any(|a| forbidden.contains(a)) {
                return reject("synthetic account in production inputs");
            }
        }
        Ok(())
    }
    pub fn allocations(&self) -> Result<Vec<Allocation>, String> {
        self.validate()?;
        let mut rows = Vec::new();
        let mut add = |category, purpose, account, amount: Balance| {
            rows.push(Allocation {
                category,
                purpose,
                account,
                base_units: amount.to_string(),
            })
        };
        for (who, amount) in self
            .beneficiaries
            .iter()
            .zip(upgrade13_policy::FOUNDING_ALLOCATION_TARGETS)
        {
            add("founding", "one-year vesting", who.clone(), amount);
        }
        add(
            "presale",
            "3-of-3 custody",
            FounderCustody::custody_account(CustodyCategory::Presale),
            20_000_000 * DECIMALS,
        );
        add(
            "liquidity",
            "3-of-3 custody",
            FounderCustody::custody_account(CustodyCategory::Liquidity),
            10_000_000 * DECIMALS,
        );
        add(
            "validator-rewards",
            "V14 staking reserve (no genesis earned liability)",
            SecurityBudget::staking_pot_account(),
            20_000_000 * DECIMALS,
        );
        // Explicit proposal for owner review: half each validator's approved bond comes from
        // founder-controlled Ecosystem, half from onboarding; all operating sums stay in Ecosystem.
        let op = DECIMALS + EXISTENTIAL_DEPOSIT;
        for v in &self.authorities {
            add(
                "ecosystem",
                "operator bond contribution including vote-rounding margin",
                v.account.clone(),
                5_000 * DECIMALS + EXISTENTIAL_DEPOSIT,
            );
            add(
                "onboarding",
                "validator onboarding support",
                v.account.clone(),
                5_000 * DECIMALS,
            );
            add(
                "ecosystem",
                "validator liquid operating balance",
                v.account.clone(),
                DECIMALS,
            );
        }
        for f in &self.founders {
            add(
                "ecosystem",
                "custody signer fees and existence",
                f.clone(),
                op,
            );
        }
        add(
            "ecosystem",
            "sudo fees and existence",
            self.sudo.clone(),
            op,
        );
        for account in [
            SecurityBudget::treasury_account(),
            SecurityBudget::fee_collection_account(),
            RewardReserve::reward_pot_account(),
        ] {
            add(
                "ecosystem",
                "protocol account existence",
                account,
                EXISTENTIAL_DEPOSIT,
            );
        }
        add(
            "ecosystem",
            "3-of-3 custody",
            FounderCustody::custody_account(CustodyCategory::Ecosystem),
            20_000_000 * DECIMALS - 20_000 * DECIMALS - 8 * op - 3 * EXISTENTIAL_DEPOSIT,
        );
        add(
            "onboarding",
            "contributor/onboarding custody",
            self.community.clone(),
            10_000_000 * DECIMALS - 20_000 * DECIMALS,
        );
        Ok(rows)
    }
    pub fn genesis(&self) -> Result<RuntimeGenesisConfig, String> {
        let mut balances: BTreeMap<AccountId, Balance> = BTreeMap::new();
        for row in self.allocations()? {
            let amount: Balance = row.base_units.parse().map_err(|_| "invalid allocation")?;
            let balance = balances.entry(row.account).or_default();
            *balance = balance.checked_add(amount).ok_or("allocation overflow")?;
        }
        if balances.values().copied().sum::<Balance>() != upgrade13_policy::TARGET_RETAINED_ISSUANCE
        {
            return Err("genesis issuance mismatch".into());
        }
        Ok(RuntimeGenesisConfig {
            balances: BalancesConfig {
                balances: balances.into_iter().collect(),
                dev_accounts: None,
            },
            sudo: SudoConfig {
                key: Some(self.sudo.clone()),
            },
            session: SessionConfig {
                keys: self
                    .authorities
                    .iter()
                    .map(|v| {
                        (
                            v.account.clone(),
                            v.account.clone(),
                            SessionKeys {
                                babe: v.babe.into(),
                                grandpa: v.grandpa.into(),
                            },
                        )
                    })
                    .collect(),
                non_authority_keys: vec![],
            },
            babe: BabeConfig {
                authorities: vec![],
                epoch_config: sp_consensus_babe::BabeEpochConfiguration {
                    c: (1, 4),
                    allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryPlainSlots,
                },
                ..Default::default()
            },
            staking: StakingConfig {
                validator_count: 4,
                minimum_validator_count: 4,
                min_validator_bond: SecurityBudgetMinimumValidatorBond::get(),
                stakers: self
                    .authorities
                    .iter()
                    .map(|v| {
                        (
                            v.account.clone(),
                            v.account.clone(),
                            SecurityBudgetMinimumValidatorBond::get() + EXISTENTIAL_DEPOSIT,
                            pallet_staking::StakerStatus::Validator,
                        )
                    })
                    .collect(),
                // Observation-only security policy is preserved; no invented invulnerable exemptions.
                invulnerables: vec![],
                ..Default::default()
            },
            fresh_genesis: crate::fresh_genesis::GenesisConfig {
                enabled: true,
                signers: self.founders.to_vec(),
                vesting_beneficiaries: self.beneficiaries.to_vec(),
            },
            ..Default::default()
        })
    }
}
