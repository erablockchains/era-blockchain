//! Non-dispatchable pinned-SDK custody derivation adapter. Coordinates are explicit inputs;
//! there are no identities, evidence approvals, account creation or transfer entry points here.
use crate::{AccountId, BlockNumber, Multisig, Proxy, ProxyType};
use era_v14_custody_governance::{CustodyCategory, StandardPalletDerivation, FOUNDER_COUNT};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PureProxyCoordinates {
    pub proxy_type: ProxyType,
    pub disambiguation_index: u16,
    pub creation_block: BlockNumber,
    pub creation_extrinsic_index: u32,
}

pub struct SdkCustodyDerivation;
impl StandardPalletDerivation<AccountId, PureProxyCoordinates> for SdkCustodyDerivation {
    fn derive_multisig(
        &self,
        sorted_signatories: &[AccountId; FOUNDER_COUNT],
        threshold: u16,
    ) -> Option<AccountId> {
        Some(Multisig::multi_account_id(sorted_signatories, threshold))
    }
    fn derive_pure_proxy(
        &self,
        controller: &AccountId,
        _category: CustodyCategory,
        coordinates: &PureProxyCoordinates,
    ) -> Option<AccountId> {
        Some(Proxy::pure_account(
            controller,
            &coordinates.proxy_type,
            coordinates.disambiguation_index,
            Some((
                coordinates.creation_block,
                coordinates.creation_extrinsic_index,
            )),
        ))
    }
}
