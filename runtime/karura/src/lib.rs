// This file is part of Acala.

// Copyright (C) 2020-2021 Acala Foundation.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! The Dev runtime. This can be compiled with `#[no_std]`, ready for Wasm.

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
// The `large_enum_variant` warning originates from `construct_runtime` macro.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::unnecessary_mut_passed)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::from_over_into)]
#![allow(clippy::upper_case_acronyms)]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

use codec::{Decode, Encode};
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata, H160};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{
		AccountIdConversion, AccountIdLookup, BadOrigin, BlakeTwo256, Block as BlockT, Convert, SaturatedConversion,
		StaticLookup, Zero,
	},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, DispatchResult, FixedPointNumber,
};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use frame_system::{EnsureRoot, RawOrigin};
use module_currencies::BasicCurrencyAdapter;
use module_evm::{CallInfo, CreateInfo};
use module_evm_accounts::EvmAddressMapping;
use module_evm_manager::EvmCurrencyIdMapping;
use module_transaction_payment::{Multiplier, TargetedFeeAdjustment};
use orml_traits::{
	create_median_value_data_provider, parameter_type_with_key, DataFeeder, DataProviderExtended, MultiCurrency,
};
use pallet_transaction_payment::RuntimeDispatchInfo;

pub use cumulus_primitives_core::ParaId;
pub use orml_xcm_support::{IsNativeConcrete, MultiCurrencyAdapter, MultiNativeAsset};
use pallet_xcm::XcmPassthrough;
pub use polkadot_parachain::primitives::Sibling;
pub use xcm::v0::{
	Junction::{self, AccountId32, GeneralKey, Parachain, Parent},
	MultiAsset,
	MultiLocation::{self, X1, X2, X3},
	NetworkId, Xcm,
};

pub use xcm_builder::{
	AccountId32Aliases, AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, EnsureXcmOrigin,
	FixedRateOfConcreteFungible, FixedWeightBounds, IsConcrete, LocationInverter, NativeAsset, ParentAsSuperuser,
	ParentIsDefault, RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeRevenue, TakeWeightCredit,
};
pub use xcm_executor::{Config, XcmExecutor};

/// Weights for pallets used in the runtime.
mod weights;

pub use frame_support::{
	construct_runtime, log, parameter_types,
	traits::{
		All, ContainsLengthBound, Currency as PalletCurrency, EnsureOrigin, Filter, Get, Imbalance, InstanceFilter,
		IsSubType, IsType, KeyOwnerProofSystem, LockIdentifier, MaxEncodedLen, OnUnbalanced, Randomness, SortedMembers,
		U128CurrencyToVote,
	},
	weights::{constants::RocksDbWeight, IdentityFee, Weight},
	PalletId, RuntimeDebug, StorageValue,
};

pub use pallet_staking::StakerStatus;
pub use pallet_timestamp::Call as TimestampCall;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
pub use sp_runtime::{Perbill, Percent, Permill, Perquintill};

pub use authority::AuthorityConfigImpl;
pub use constants::{fee::*, homa::*, time::*};
pub use primitives::{
	evm::EstimateResourcesRequest, AccountId, AccountIndex, Amount, AuctionId, AuthoritysOriginId, Balance,
	BlockNumber, CurrencyId, DataProviderId, EraIndex, Hash, Moment, Nonce, ReserveIdentifier, Share, Signature,
	TokenSymbol, TradingPair,
};
pub use runtime_common::{
	cent, dollar, microcent, millicent, CurveFeeModel, EnsureRootOrAllGeneralCouncil,
	EnsureRootOrAllTechnicalCommittee, EnsureRootOrHalfFinancialCouncil, EnsureRootOrHalfGeneralCouncil,
	EnsureRootOrHalfHomaCouncil, EnsureRootOrOneThirdsTechnicalCommittee, EnsureRootOrThreeFourthsGeneralCouncil,
	EnsureRootOrTwoThirdsGeneralCouncil, EnsureRootOrTwoThirdsTechnicalCommittee, ExchangeRate,
	FinancialCouncilInstance, FinancialCouncilMembershipInstance, GasToWeight, GeneralCouncilInstance,
	GeneralCouncilMembershipInstance, HomaCouncilInstance, HomaCouncilMembershipInstance,
	OperatorMembershipInstanceAcala, OperatorMembershipInstanceBand, Price, ProxyType, Rate, Ratio,
	RelaychainBlockNumberProvider, RuntimeBlockLength, RuntimeBlockWeights, SystemContractsFilter,
	TechnicalCommitteeInstance, TechnicalCommitteeMembershipInstance, TimeStampedPrice, KAR, KSM, KUSD, LKSM, RENBTC,
};

mod authority;
mod benchmarking;
mod constants;

/// This runtime version.
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("karura"),
	impl_name: create_runtime_str!("karura"),
	authoring_version: 1,
	spec_version: 1006,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
};

/// The version infromation used to identify this runtime when compiled
/// natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion {
		runtime_version: VERSION,
		can_author_with: Default::default(),
	}
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

// Pallet accounts of runtime
parameter_types! {
	pub const TreasuryPalletId: PalletId = PalletId(*b"aca/trsy");
	pub const LoansPalletId: PalletId = PalletId(*b"aca/loan");
	pub const DEXPalletId: PalletId = PalletId(*b"aca/dexm");
	pub const CDPTreasuryPalletId: PalletId = PalletId(*b"aca/cdpt");
	pub const StakingPoolPalletId: PalletId = PalletId(*b"aca/stkp");
	pub const HonzonTreasuryPalletId: PalletId = PalletId(*b"aca/hztr");
	pub const HomaTreasuryPalletId: PalletId = PalletId(*b"aca/hmtr");
	pub const IncentivesPalletId: PalletId = PalletId(*b"aca/inct");
	pub const CollatorPotId: PalletId = PalletId(*b"aca/cpot");
	// Treasury reserve
	pub const TreasuryReservePalletId: PalletId = PalletId(*b"aca/reve");
	pub const NftPalletId: PalletId = PalletId(*b"aca/aNFT");
	// Vault all unrleased native token.
	pub UnreleasedNativeVaultAccountId: AccountId = PalletId(*b"aca/urls").into_account();
}

pub fn get_all_module_accounts() -> Vec<AccountId> {
	vec![
		TreasuryPalletId::get().into_account(),
		LoansPalletId::get().into_account(),
		DEXPalletId::get().into_account(),
		CDPTreasuryPalletId::get().into_account(),
		StakingPoolPalletId::get().into_account(),
		HonzonTreasuryPalletId::get().into_account(),
		HomaTreasuryPalletId::get().into_account(),
		IncentivesPalletId::get().into_account(),
		CollatorPotId::get().into_account(),
		TreasuryReservePalletId::get().into_account(),
		ZeroAccountId::get(),
	]
}

parameter_types! {
	pub const BlockHashCount: BlockNumber = 1200; // mortal tx can be valid up to 4 hour after signing
	pub const Version: RuntimeVersion = VERSION;
	pub const SS58Prefix: u8 = 8; // Ss58AddressFormat::KaruraAccount
}

pub struct BaseCallFilter;
impl Filter<Call> for BaseCallFilter {
	fn filter(call: &Call) -> bool {
		matches!(
			call,
			// Core
			Call::System(_) | Call::Timestamp(_) | Call::ParachainSystem(_) |
			// Utility
			Call::Scheduler(_) | Call::Utility(_) | Call::Multisig(_) | Call::Proxy(_) |
			// Councils
			Call::Authority(_) | Call::GeneralCouncil(_) | Call::GeneralCouncilMembership(_) |
			Call::FinancialCouncil(_) | Call::FinancialCouncilMembership(_) |
			Call::HomaCouncil(_) | Call::HomaCouncilMembership(_) |
			Call::TechnicalCommittee(_) | Call::TechnicalCommitteeMembership(_) |
			// Oracle
			Call::AcalaOracle(_) | Call::OperatorMembershipAcala(_) |
			// Democracy
			Call::Democracy(_) | Call::Treasury(_) | Call::Bounties(_) | Call::Tips(_) |
			// Collactor Selection
			Call::CollatorSelection(_) | Call::Session(_) | Call::SessionManager(_) |
			// Vesting
			Call::Vesting(_) |
			// TransactionPayment
			Call::TransactionPayment(_) |
			// Tokens
			Call::XTokens(_) | Call::Balances(_) | Call::Currencies(_) |
			// NFT
			Call::NFT(_) |
			// DEX
			Call::Dex(_) |
			// Incentives
			Call::Incentives(_) |
			// Honzon
			Call::Auction(_) | Call::AuctionManager(_) | Call::Honzon(_) | Call::Loans(_) | Call::Prices(_) |
			Call::CdpTreasury(_) | Call::CdpEngine(_) | Call::EmergencyShutdown(_)
		)
	}
}

impl frame_system::Config for Runtime {
	type AccountId = AccountId;
	type Call = Call;
	type Lookup = AccountIdLookup<AccountId, AccountIndex>;
	type Index = Nonce;
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hashing = BlakeTwo256;
	type Header = generic::Header<BlockNumber, BlakeTwo256>;
	type Event = Event;
	type Origin = Origin;
	type BlockHashCount = BlockHashCount;
	type BlockWeights = RuntimeBlockWeights;
	type BlockLength = RuntimeBlockLength;
	type Version = Version;
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = (
		module_evm::CallKillAccount<Runtime>,
		module_evm_accounts::CallKillAccount<Runtime>,
	);
	type DbWeight = RocksDbWeight;
	type BaseCallFilter = BaseCallFilter;
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
}

parameter_types! {
	pub const UncleGenerations: u32 = 0;
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
	type UncleGenerations = UncleGenerations;
	type FilterUncle = ();
	type EventHandler = CollatorSelection;
}

parameter_types! {
	pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
	pub const Period: u32 = 6 * HOURS;
}

impl pallet_session::Config for Runtime {
	type Event = Event;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	// we don't have stash and controller, thus we don't need the convert as well.
	type ValidatorIdOf = module_collator_selection::IdentityCollator;
	type ShouldEndSession = SessionManager;
	type NextSessionRotation = SessionManager;
	type SessionManager = CollatorSelection;
	// Essentially just Aura, but lets be pedantic.
	type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
	type WeightInfo = ();
}

parameter_types! {
	pub const MinCandidates: u32 = 4;
	pub const MaxCandidates: u32 = 50;
	pub const MaxInvulnerables: u32 = 10;
	pub const KickPenaltySessionLength: u32 = 8;
	pub const CollatorKickThreshold: Permill = Permill::from_percent(30);
}

impl module_collator_selection::Config for Runtime {
	type Event = Event;
	type Currency = Balances;
	type ValidatorSet = Session;
	type UpdateOrigin = EnsureRootOrHalfGeneralCouncil;
	type PotId = CollatorPotId;
	type MinCandidates = MinCandidates;
	type MaxCandidates = MaxCandidates;
	type MaxInvulnerables = MaxInvulnerables;
	type KickPenaltySessionLength = KickPenaltySessionLength;
	type CollatorKickThreshold = CollatorKickThreshold;
	type WeightInfo = weights::module_collator_selection::WeightInfo<Runtime>;
}

parameter_types! {
	pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = Moment;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

parameter_types! {
	pub NativeTokenExistentialDeposit: Balance = 10 * cent(KAR);	// 0.1 KAR
	// For weight estimation, we assume that the most locks on an individual account will be 50.
	// This number may need to be adjusted in the future if this assumption no longer holds true.
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = ReserveIdentifier::Count as u32;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = Treasury;
	type Event = Event;
	type ExistentialDeposit = NativeTokenExistentialDeposit;
	type AccountStore = frame_system::Pallet<Runtime>;
	type MaxLocks = MaxLocks;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = ReserveIdentifier;
	type WeightInfo = ();
}

parameter_types! {
	pub TransactionByteFee: Balance = millicent(KAR);
	/// The portion of the `NORMAL_DISPATCH_RATIO` that we adjust the fees with. Blocks filled less
	/// than this will decrease the weight and more will increase.
	pub const TargetBlockFullness: Perquintill = Perquintill::from_percent(25);
	/// The adjustment variable of the runtime. Higher values will cause `TargetBlockFullness` to
	/// change the fees more rapidly.
	pub AdjustmentVariable: Multiplier = Multiplier::saturating_from_rational(3, 100_000);
	/// Minimum amount of the multiplier. This value cannot be too low. A test case should ensure
	/// that combined with `AdjustmentVariable`, we can recover from the minimum.
	/// See `multiplier_can_grow_from_zero`.
	pub MinimumMultiplier: Multiplier = Multiplier::saturating_from_rational(1, 1_000_000u128);
}

pub type SlowAdjustingFeeUpdate<R> =
	TargetedFeeAdjustment<R, TargetBlockFullness, AdjustmentVariable, MinimumMultiplier>;

impl pallet_sudo::Config for Runtime {
	type Event = Event;
	type Call = Call;
}

parameter_types! {
	pub const GeneralCouncilMotionDuration: BlockNumber = 3 * DAYS;
	pub const GeneralCouncilMaxProposals: u32 = 50;
	pub const GeneralCouncilMaxMembers: u32 = 50;
}

impl pallet_collective::Config<GeneralCouncilInstance> for Runtime {
	type Origin = Origin;
	type Proposal = Call;
	type Event = Event;
	type MotionDuration = GeneralCouncilMotionDuration;
	type MaxProposals = GeneralCouncilMaxProposals;
	type MaxMembers = GeneralCouncilMaxMembers;
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	type WeightInfo = ();
}

impl pallet_membership::Config<GeneralCouncilMembershipInstance> for Runtime {
	type Event = Event;
	type AddOrigin = EnsureRootOrThreeFourthsGeneralCouncil;
	type RemoveOrigin = EnsureRootOrThreeFourthsGeneralCouncil;
	type SwapOrigin = EnsureRootOrThreeFourthsGeneralCouncil;
	type ResetOrigin = EnsureRootOrThreeFourthsGeneralCouncil;
	type PrimeOrigin = EnsureRootOrThreeFourthsGeneralCouncil;
	type MembershipInitialized = GeneralCouncil;
	type MembershipChanged = GeneralCouncil;
	type MaxMembers = GeneralCouncilMaxMembers;
	type WeightInfo = ();
}

parameter_types! {
	pub const FinancialCouncilMotionDuration: BlockNumber = 3 * DAYS;
	pub const FinancialCouncilMaxProposals: u32 = 50;
	pub const FinancialCouncilMaxMembers: u32 = 50;
}

impl pallet_collective::Config<FinancialCouncilInstance> for Runtime {
	type Origin = Origin;
	type Proposal = Call;
	type Event = Event;
	type MotionDuration = FinancialCouncilMotionDuration;
	type MaxProposals = FinancialCouncilMaxProposals;
	type MaxMembers = FinancialCouncilMaxMembers;
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	type WeightInfo = ();
}

impl pallet_membership::Config<FinancialCouncilMembershipInstance> for Runtime {
	type Event = Event;
	type AddOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type RemoveOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type SwapOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type ResetOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type PrimeOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type MembershipInitialized = FinancialCouncil;
	type MembershipChanged = FinancialCouncil;
	type MaxMembers = FinancialCouncilMaxMembers;
	type WeightInfo = ();
}

parameter_types! {
	pub const HomaCouncilMotionDuration: BlockNumber = 3 * DAYS;
	pub const HomaCouncilMaxProposals: u32 = 50;
	pub const HomaCouncilMaxMembers: u32 = 50;
}

impl pallet_collective::Config<HomaCouncilInstance> for Runtime {
	type Origin = Origin;
	type Proposal = Call;
	type Event = Event;
	type MotionDuration = HomaCouncilMotionDuration;
	type MaxProposals = HomaCouncilMaxProposals;
	type MaxMembers = HomaCouncilMaxMembers;
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	type WeightInfo = ();
}

impl pallet_membership::Config<HomaCouncilMembershipInstance> for Runtime {
	type Event = Event;
	type AddOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type RemoveOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type SwapOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type ResetOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type PrimeOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type MembershipInitialized = HomaCouncil;
	type MembershipChanged = HomaCouncil;
	type MaxMembers = HomaCouncilMaxMembers;
	type WeightInfo = ();
}

parameter_types! {
	pub const TechnicalCommitteeMotionDuration: BlockNumber = 3 * DAYS;
	pub const TechnicalCommitteeMaxProposals: u32 = 50;
	pub const TechnicalCouncilMaxMembers: u32 = 50;
}

impl pallet_collective::Config<TechnicalCommitteeInstance> for Runtime {
	type Origin = Origin;
	type Proposal = Call;
	type Event = Event;
	type MotionDuration = TechnicalCommitteeMotionDuration;
	type MaxProposals = TechnicalCommitteeMaxProposals;
	type MaxMembers = TechnicalCouncilMaxMembers;
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	type WeightInfo = ();
}

impl pallet_membership::Config<TechnicalCommitteeMembershipInstance> for Runtime {
	type Event = Event;
	type AddOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type RemoveOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type SwapOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type ResetOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type PrimeOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type MembershipInitialized = TechnicalCommittee;
	type MembershipChanged = TechnicalCommittee;
	type MaxMembers = TechnicalCouncilMaxMembers;
	type WeightInfo = ();
}

parameter_types! {
	pub const OracleMaxMembers: u32 = 50;
}

impl pallet_membership::Config<OperatorMembershipInstanceAcala> for Runtime {
	type Event = Event;
	type AddOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type RemoveOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type SwapOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type ResetOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type PrimeOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type MembershipInitialized = ();
	type MembershipChanged = AcalaOracle;
	type MaxMembers = OracleMaxMembers;
	type WeightInfo = ();
}

impl pallet_utility::Config for Runtime {
	type Event = Event;
	type Call = Call;
	type WeightInfo = ();
}

parameter_types! {
	pub MultisigDepositBase: Balance = deposit(1, 88);
	pub MultisigDepositFactor: Balance = deposit(0, 32);
	pub const MaxSignatories: u16 = 100;
}

impl pallet_multisig::Config for Runtime {
	type Event = Event;
	type Call = Call;
	type Currency = Balances;
	type DepositBase = MultisigDepositBase;
	type DepositFactor = MultisigDepositFactor;
	type MaxSignatories = MaxSignatories;
	type WeightInfo = ();
}

pub struct GeneralCouncilProvider;
impl SortedMembers<AccountId> for GeneralCouncilProvider {
	fn contains(who: &AccountId) -> bool {
		GeneralCouncil::is_member(who)
	}

	fn sorted_members() -> Vec<AccountId> {
		GeneralCouncil::members()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add(_: &AccountId) {
		unimplemented!()
	}
}

impl ContainsLengthBound for GeneralCouncilProvider {
	fn max_len() -> usize {
		GeneralCouncilMaxMembers::get() as usize
	}
	fn min_len() -> usize {
		0
	}
}

parameter_types! {
	pub const ProposalBond: Permill = Permill::from_percent(5);
	pub ProposalBondMinimum: Balance = 5 * dollar(KAR);
	pub const SpendPeriod: BlockNumber = 7 * DAYS;
	pub const Burn: Permill = Permill::from_percent(0);

	pub const TipCountdown: BlockNumber = DAYS;
	pub const TipFindersFee: Percent = Percent::from_percent(5);
	pub TipReportDepositBase: Balance = deposit(1, 0);
	pub BountyDepositBase: Balance = deposit(1, 0);
	pub const BountyDepositPayoutDelay: BlockNumber = 4 * DAYS;
	pub const BountyUpdatePeriod: BlockNumber = 35 * DAYS;
	pub const BountyCuratorDeposit: Permill = Permill::from_percent(50);
	pub BountyValueMinimum: Balance = 5 * dollar(KAR);
	pub DataDepositPerByte: Balance = deposit(0, 1);
	pub const MaximumReasonLength: u32 = 8192;
	pub const MaxApprovals: u32 = 30;
}

impl pallet_treasury::Config for Runtime {
	type PalletId = TreasuryPalletId;
	type Currency = Balances;
	type ApproveOrigin = EnsureRootOrHalfGeneralCouncil;
	type RejectOrigin = EnsureRootOrHalfGeneralCouncil;
	type Event = Event;
	type OnSlash = Treasury;
	type ProposalBond = ProposalBond;
	type ProposalBondMinimum = ProposalBondMinimum;
	type SpendPeriod = SpendPeriod;
	type Burn = Burn;
	type BurnDestination = ();
	type SpendFunds = Bounties;
	type WeightInfo = ();
	type MaxApprovals = MaxApprovals;
}

impl pallet_bounties::Config for Runtime {
	type Event = Event;
	type BountyDepositBase = BountyDepositBase;
	type BountyDepositPayoutDelay = BountyDepositPayoutDelay;
	type BountyUpdatePeriod = BountyUpdatePeriod;
	type BountyCuratorDeposit = BountyCuratorDeposit;
	type BountyValueMinimum = BountyValueMinimum;
	type DataDepositPerByte = DataDepositPerByte;
	type MaximumReasonLength = MaximumReasonLength;
	type WeightInfo = ();
}

impl pallet_tips::Config for Runtime {
	type Event = Event;
	type DataDepositPerByte = DataDepositPerByte;
	type MaximumReasonLength = MaximumReasonLength;
	type Tippers = GeneralCouncilProvider;
	type TipCountdown = TipCountdown;
	type TipFindersFee = TipFindersFee;
	type TipReportDepositBase = TipReportDepositBase;
	type WeightInfo = ();
}

parameter_types! {
	pub const LaunchPeriod: BlockNumber = 2 * DAYS;
	pub const VotingPeriod: BlockNumber = 2 * DAYS;
	pub const FastTrackVotingPeriod: BlockNumber = 3 * HOURS;
	pub MinimumDeposit: Balance = 1000 * dollar(KAR);
	pub const EnactmentPeriod: BlockNumber = 36 * HOURS;
	pub const CooloffPeriod: BlockNumber = 7 * DAYS;
	pub PreimageByteDeposit: Balance = deposit(0, 1);
	pub const InstantAllowed: bool = true;
	pub const MaxVotes: u32 = 100;
	pub const MaxProposals: u32 = 100;
}

impl pallet_democracy::Config for Runtime {
	type Proposal = Call;
	type Event = Event;
	type Currency = Balances;
	type EnactmentPeriod = EnactmentPeriod;
	type LaunchPeriod = LaunchPeriod;
	type VotingPeriod = VotingPeriod;
	type MinimumDeposit = MinimumDeposit;
	/// A straight majority of the council can decide what their next motion is.
	type ExternalOrigin = EnsureRootOrHalfGeneralCouncil;
	/// A majority can have the next scheduled referendum be a straight majority-carries vote.
	type ExternalMajorityOrigin = EnsureRootOrHalfGeneralCouncil;
	/// A unanimous council can have the next scheduled referendum be a straight default-carries
	/// (NTB) vote.
	type ExternalDefaultOrigin = EnsureRootOrAllGeneralCouncil;
	/// Two thirds of the technical committee can have an ExternalMajority/ExternalDefault vote
	/// be tabled immediately and with a shorter voting/enactment period.
	type FastTrackOrigin = EnsureRootOrTwoThirdsTechnicalCommittee;
	type InstantOrigin = EnsureRootOrAllTechnicalCommittee;
	type InstantAllowed = InstantAllowed;
	type FastTrackVotingPeriod = FastTrackVotingPeriod;
	// To cancel a proposal which has been passed, 2/3 of the council must agree to it.
	type CancellationOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type BlacklistOrigin = EnsureRoot<AccountId>;
	// To cancel a proposal before it has been passed, the technical committee must be unanimous or
	// Root must agree.
	type CancelProposalOrigin = EnsureRootOrAllTechnicalCommittee;
	// Any single technical committee member may veto a coming council proposal, however they can
	// only do it once and it lasts only for the cooloff period.
	type VetoOrigin = pallet_collective::EnsureMember<AccountId, TechnicalCommitteeInstance>;
	type CooloffPeriod = CooloffPeriod;
	type PreimageByteDeposit = PreimageByteDeposit;
	type OperationalPreimageOrigin = pallet_collective::EnsureMember<AccountId, GeneralCouncilInstance>;
	type Slash = Treasury;
	type Scheduler = Scheduler;
	type PalletsOrigin = OriginCaller;
	type MaxVotes = MaxVotes;
	//TODO: might need to weight for Karura
	type WeightInfo = pallet_democracy::weights::SubstrateWeight<Runtime>;
	type MaxProposals = MaxProposals;
}

impl orml_auction::Config for Runtime {
	type Event = Event;
	type Balance = Balance;
	type AuctionId = AuctionId;
	type Handler = AuctionManager;
	type WeightInfo = weights::orml_auction::WeightInfo<Runtime>;
}

impl orml_authority::Config for Runtime {
	type Event = Event;
	type Origin = Origin;
	type PalletsOrigin = OriginCaller;
	type Call = Call;
	type Scheduler = Scheduler;
	type AsOriginId = AuthoritysOriginId;
	type AuthorityConfig = AuthorityConfigImpl;
	type WeightInfo = weights::orml_authority::WeightInfo<Runtime>;
}

parameter_types! {
	pub const MinimumCount: u32 = 5;
	pub const ExpiresIn: Moment = 1000 * 60 * 60; // 1 hours
	pub ZeroAccountId: AccountId = AccountId::from([0u8; 32]);
	pub const MaxHasDispatchedSize: u32 = 20;
}

type AcalaDataProvider = orml_oracle::Instance1;
impl orml_oracle::Config<AcalaDataProvider> for Runtime {
	type Event = Event;
	type OnNewData = ();
	type CombineData = orml_oracle::DefaultCombineData<Runtime, MinimumCount, ExpiresIn, AcalaDataProvider>;
	type Time = Timestamp;
	type OracleKey = CurrencyId;
	type OracleValue = Price;
	type RootOperatorAccountId = ZeroAccountId;
	type Members = OperatorMembershipAcala;
	type MaxHasDispatchedSize = MaxHasDispatchedSize;
	type WeightInfo = ();
}

create_median_value_data_provider!(
	AggregatedDataProvider,
	CurrencyId,
	Price,
	TimeStampedPrice,
	[AcalaOracle]
);
// Aggregated data provider cannot feed.
impl DataFeeder<CurrencyId, Price, AccountId> for AggregatedDataProvider {
	fn feed_value(_: AccountId, _: CurrencyId, _: Price) -> DispatchResult {
		Err("Not supported".into())
	}
}

parameter_type_with_key! {
	pub ExistentialDeposits: |currency_id: CurrencyId| -> Balance {
		match currency_id {
			CurrencyId::Token(symbol) => match symbol {
				TokenSymbol::KUSD => cent(*currency_id),
				TokenSymbol::KSM => 10 * millicent(*currency_id),
				TokenSymbol::LKSM => 50 * millicent(*currency_id),

				TokenSymbol::ACA |
				TokenSymbol::AUSD |
				TokenSymbol::DOT |
				TokenSymbol::LDOT |
				TokenSymbol::RENBTC |
				TokenSymbol::KAR |
				TokenSymbol::CASH => Balance::max_value() // unsupported
			},
			CurrencyId::DexShare(dex_share_0, _) => {
				let currency_id_0: CurrencyId = (*dex_share_0).into();

				// initial dex share amount is calculated based on currency_id_0,
				// use the ED of currency_id_0 as the ED of lp token.
				if currency_id_0 == GetNativeCurrencyId::get() {
					NativeTokenExistentialDeposit::get()
				} else {
					Self::get(&currency_id_0)
				}
			},
			CurrencyId::Erc20(_) => Balance::max_value(), // not handled by orml-tokens
			CurrencyId::ChainSafe(_) => Balance::max_value(), // TODO: update this before we enable ChainSafe bridge
		}
	};
}

parameter_types! {
	pub KaruraTreasuryAccount: AccountId = TreasuryPalletId::get().into_account();
}

impl orml_tokens::Config for Runtime {
	type Event = Event;
	type Balance = Balance;
	type Amount = Amount;
	type CurrencyId = CurrencyId;
	type WeightInfo = weights::orml_tokens::WeightInfo<Runtime>;
	type ExistentialDeposits = ExistentialDeposits;
	type OnDust = orml_tokens::TransferDust<Runtime, KaruraTreasuryAccount>;
	type MaxLocks = MaxLocks;
}

parameter_types! {
	pub StableCurrencyFixedPrice: Price = Price::saturating_from_rational(1, 1);
}

impl module_prices::Config for Runtime {
	type Event = Event;
	type Source = AggregatedDataProvider;
	type GetStableCurrencyId = GetStableCurrencyId;
	type StableCurrencyFixedPrice = StableCurrencyFixedPrice;
	type GetStakingCurrencyId = GetStakingCurrencyId;
	type GetLiquidCurrencyId = GetLiquidCurrencyId;
	type LockOrigin = EnsureRootOrTwoThirdsGeneralCouncil;
	type LiquidStakingExchangeRateProvider = LiquidStakingExchangeRateProvider;
	type DEX = Dex;
	type Currency = Currencies;
	type CurrencyIdMapping = EvmCurrencyIdMapping<Runtime>;
	type WeightInfo = weights::module_prices::WeightInfo<Runtime>;
}

pub struct LiquidStakingExchangeRateProvider;
impl module_support::ExchangeRateProvider for LiquidStakingExchangeRateProvider {
	fn get_exchange_rate() -> ExchangeRate {
		ExchangeRate::zero()
	}
}

parameter_types! {
	pub const GetNativeCurrencyId: CurrencyId = KAR;
	pub const GetStableCurrencyId: CurrencyId = KUSD;
	pub const GetLiquidCurrencyId: CurrencyId = LKSM;
	pub const GetStakingCurrencyId: CurrencyId = KSM;
}

impl module_currencies::Config for Runtime {
	type Event = Event;
	type MultiCurrency = Tokens;
	type NativeCurrency = BasicCurrencyAdapter<Runtime, Balances, Amount, BlockNumber>;
	type GetNativeCurrencyId = GetNativeCurrencyId;
	type WeightInfo = weights::module_currencies::WeightInfo<Runtime>;
	type AddressMapping = EvmAddressMapping<Runtime>;
	type EVMBridge = EVMBridge;
}

parameter_types! {
	pub KaruraFoundationAccounts: Vec<AccountId> = vec![
		hex_literal::hex!["efd29d0d6e63911ae3727fc71506bc3365c5d3b39e3a1680c857b4457cf8afad"].into(),	// tij5W2NzmtxxAbwudwiZpif9ScmZfgFYdzrJWKYq6oNbSNH
		hex_literal::hex!["41dd2515ea11692c02306b68a2c6ff69b6606ebddaac40682789cfab300971c4"].into(),	// pndshZqDAC9GutDvv7LzhGhgWeGv5YX9puFA8xDidHXCyjd
	];
}

pub struct EnsureKaruraFoundation;
impl EnsureOrigin<Origin> for EnsureKaruraFoundation {
	type Success = AccountId;

	fn try_origin(o: Origin) -> Result<Self::Success, Origin> {
		Into::<Result<RawOrigin<AccountId>, Origin>>::into(o).and_then(|o| match o {
			RawOrigin::Signed(caller) => {
				if KaruraFoundationAccounts::get().contains(&caller) {
					Ok(caller)
				} else {
					Err(Origin::from(Some(caller)))
				}
			}
			r => Err(Origin::from(r)),
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> Origin {
		Origin::from(RawOrigin::Signed(Default::default()))
	}
}

parameter_types! {
	pub MinVestedTransfer: Balance = 0;
	pub const MaxVestingSchedules: u32 = 100;
}

impl orml_vesting::Config for Runtime {
	type Event = Event;
	type Currency = pallet_balances::Pallet<Runtime>;
	type MinVestedTransfer = MinVestedTransfer;
	type VestedTransferOrigin = EnsureKaruraFoundation;
	type WeightInfo = weights::orml_vesting::WeightInfo<Runtime>;
	type MaxVestingSchedules = MaxVestingSchedules;
	type BlockNumberProvider = RelaychainBlockNumberProvider<Runtime>;
}

parameter_types! {
	pub MaximumSchedulerWeight: Weight = Perbill::from_percent(10) * RuntimeBlockWeights::get().max_block;
	pub const MaxScheduledPerBlock: u32 = 30;
}

impl pallet_scheduler::Config for Runtime {
	type Event = Event;
	type Origin = Origin;
	type PalletsOrigin = OriginCaller;
	type Call = Call;
	type MaximumWeight = MaximumSchedulerWeight;
	type ScheduleOrigin = EnsureRoot<AccountId>;
	type MaxScheduledPerBlock = MaxScheduledPerBlock;
	type WeightInfo = ();
}

parameter_types! {
	pub MinimumIncrementSize: Rate = Rate::saturating_from_rational(2, 100);
	pub const AuctionTimeToClose: BlockNumber = 15 * MINUTES;
	pub const AuctionDurationSoftCap: BlockNumber = 2 * HOURS;
}

impl module_auction_manager::Config for Runtime {
	type Event = Event;
	type Currency = Currencies;
	type Auction = Auction;
	type MinimumIncrementSize = MinimumIncrementSize;
	type AuctionTimeToClose = AuctionTimeToClose;
	type AuctionDurationSoftCap = AuctionDurationSoftCap;
	type GetStableCurrencyId = GetStableCurrencyId;
	type CDPTreasury = CdpTreasury;
	type DEX = Dex;
	type PriceSource = module_prices::PriorityLockedPriceProvider<Runtime>;
	type UnsignedPriority = runtime_common::AuctionManagerUnsignedPriority;
	type EmergencyShutdown = EmergencyShutdown;
	type WeightInfo = weights::module_auction_manager::WeightInfo<Runtime>;
}

impl module_loans::Config for Runtime {
	type Event = Event;
	type Convert = module_cdp_engine::DebitExchangeRateConvertor<Runtime>;
	type Currency = Currencies;
	type RiskManager = CdpEngine;
	type CDPTreasury = CdpTreasury;
	type PalletId = LoansPalletId;
	type OnUpdateLoan = module_incentives::OnUpdateLoan<Runtime>;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
where
	Call: From<LocalCall>,
{
	fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
		call: Call,
		public: <Signature as sp_runtime::traits::Verify>::Signer,
		account: AccountId,
		nonce: Nonce,
	) -> Option<(
		Call,
		<UncheckedExtrinsic as sp_runtime::traits::Extrinsic>::SignaturePayload,
	)> {
		// take the biggest period possible.
		let period = BlockHashCount::get()
			.checked_next_power_of_two()
			.map(|c| c / 2)
			.unwrap_or(2) as u64;
		let current_block = System::block_number()
			.saturated_into::<u64>()
			// The `System::block_number` is initialized with `n+1`,
			// so the actual block number is `n`.
			.saturating_sub(1);
		let tip = 0;
		let extra: SignedExtra = (
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(generic::Era::mortal(period, current_block)),
			frame_system::CheckNonce::<Runtime>::from(nonce),
			frame_system::CheckWeight::<Runtime>::new(),
			module_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
			module_evm::SetEvmOrigin::<Runtime>::new(),
		);
		let raw_payload = SignedPayload::new(call, extra)
			.map_err(|e| {
				log::warn!("Unable to create signed payload: {:?}", e);
			})
			.ok()?;
		let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
		let address = AccountIdLookup::unlookup(account);
		let (call, extra, _) = raw_payload.deconstruct();
		Some((call, (address, signature, extra)))
	}
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as sp_runtime::traits::Verify>::Signer;
	type Signature = Signature;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
	Call: From<C>,
{
	type OverarchingCall = Call;
	type Extrinsic = UncheckedExtrinsic;
}

parameter_types! {
	pub CollateralCurrencyIds: Vec<CurrencyId> = vec![KSM, LKSM];
	pub DefaultLiquidationRatio: Ratio = Ratio::saturating_from_rational(150, 100);
	pub DefaultDebitExchangeRate: ExchangeRate = ExchangeRate::saturating_from_rational(1, 10);
	pub DefaultLiquidationPenalty: Rate = Rate::saturating_from_rational(8, 100);
	pub MinimumDebitValue: Balance = 20 * dollar(KUSD);
	pub MaxSwapSlippageCompareToOracle: Ratio = Ratio::saturating_from_rational(1, 100);
}

impl module_cdp_engine::Config for Runtime {
	type Event = Event;
	type PriceSource = module_prices::PriorityLockedPriceProvider<Runtime>;
	type CollateralCurrencyIds = CollateralCurrencyIds;
	type DefaultLiquidationRatio = DefaultLiquidationRatio;
	type DefaultDebitExchangeRate = DefaultDebitExchangeRate;
	type DefaultLiquidationPenalty = DefaultLiquidationPenalty;
	type MinimumDebitValue = MinimumDebitValue;
	type GetStableCurrencyId = GetStableCurrencyId;
	type CDPTreasury = CdpTreasury;
	type UpdateOrigin = EnsureRootOrHalfFinancialCouncil;
	type MaxSwapSlippageCompareToOracle = MaxSwapSlippageCompareToOracle;
	type UnsignedPriority = runtime_common::CdpEngineUnsignedPriority;
	type EmergencyShutdown = EmergencyShutdown;
	type UnixTime = Timestamp;
	type WeightInfo = weights::module_cdp_engine::WeightInfo<Runtime>;
}

parameter_types! {
	pub DepositPerAuthorization: Balance = deposit(1, 64);
}

impl module_honzon::Config for Runtime {
	type Event = Event;
	type Currency = Balances;
	type DepositPerAuthorization = DepositPerAuthorization;
	type WeightInfo = weights::module_honzon::WeightInfo<Runtime>;
}

impl module_emergency_shutdown::Config for Runtime {
	type Event = Event;
	type CollateralCurrencyIds = CollateralCurrencyIds;
	type PriceSource = Prices;
	type CDPTreasury = CdpTreasury;
	type AuctionManagerHandler = AuctionManager;
	type ShutdownOrigin = EnsureRoot<AccountId>;
	type WeightInfo = weights::module_emergency_shutdown::WeightInfo<Runtime>;
}

parameter_types! {
	pub const GetExchangeFee: (u32, u32) = (3, 1000);	// 0.3%
	pub const TradingPathLimit: u32 = 3;
}

impl module_dex::Config for Runtime {
	type Event = Event;
	type Currency = Currencies;
	type GetExchangeFee = GetExchangeFee;
	type TradingPathLimit = TradingPathLimit;
	type PalletId = DEXPalletId;
	type CurrencyIdMapping = EvmCurrencyIdMapping<Runtime>;
	type DEXIncentives = Incentives;
	type WeightInfo = weights::module_dex::WeightInfo<Runtime>;
	type ListingOrigin = EnsureRootOrHalfGeneralCouncil;
}

parameter_types! {
	pub const MaxAuctionsCount: u32 = 100;
	pub HonzonTreasuryAccount: AccountId = HonzonTreasuryPalletId::get().into_account();
}

impl module_cdp_treasury::Config for Runtime {
	type Event = Event;
	type Currency = Currencies;
	type GetStableCurrencyId = GetStableCurrencyId;
	type AuctionManagerHandler = AuctionManager;
	type UpdateOrigin = EnsureRootOrHalfFinancialCouncil;
	type DEX = Dex;
	type MaxAuctionsCount = MaxAuctionsCount;
	type PalletId = CDPTreasuryPalletId;
	type TreasuryAccount = HonzonTreasuryAccount;
	type WeightInfo = weights::module_cdp_treasury::WeightInfo<Runtime>;
}

parameter_types! {
	// Sort by fee charge order
	pub DefaultFeeSwapPathList: Vec<Vec<CurrencyId>> = vec![vec![KUSD, KAR], vec![KSM, KAR], vec![LKSM, KAR]];
}

type NegativeImbalance = <Balances as PalletCurrency<AccountId>>::NegativeImbalance;
pub struct DealWithFees;
impl OnUnbalanced<NegativeImbalance> for DealWithFees {
	fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance>) {
		if let Some(mut fees) = fees_then_tips.next() {
			if let Some(tips) = fees_then_tips.next() {
				tips.merge_into(&mut fees);
			}
			// for fees and tips, 100% to treasury
			Treasury::on_unbalanced(fees);
		}
	}
}

impl module_transaction_payment::Config for Runtime {
	type NativeCurrencyId = GetNativeCurrencyId;
	type DefaultFeeSwapPathList = DefaultFeeSwapPathList;
	type Currency = Balances;
	type MultiCurrency = Currencies;
	type OnTransactionPayment = DealWithFees;
	type TransactionByteFee = TransactionByteFee;
	type WeightToFee = WeightToFee;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
	type DEX = Dex;
	type MaxSwapSlippageCompareToOracle = MaxSwapSlippageCompareToOracle;
	type TradingPathLimit = TradingPathLimit;
	type PriceSource = module_prices::RealTimePriceProvider<Runtime>;
	type WeightInfo = weights::module_transaction_payment::WeightInfo<Runtime>;
}

impl module_evm_accounts::Config for Runtime {
	type Event = Event;
	type Currency = Balances;
	type AddressMapping = EvmAddressMapping<Runtime>;
	type TransferAll = Currencies;
	type WeightInfo = weights::module_evm_accounts::WeightInfo<Runtime>;
}

impl module_evm_manager::Config for Runtime {
	type Currency = Balances;
	type EVMBridge = EVMBridge;
}

impl orml_rewards::Config for Runtime {
	type Share = Balance;
	type Balance = Balance;
	type PoolId = module_incentives::PoolId<AccountId>;
	type Handler = Incentives;
}

parameter_types! {
	pub const AccumulatePeriod: BlockNumber = MINUTES;
}

impl module_incentives::Config for Runtime {
	type Event = Event;
	type RelaychainAccountId = AccountId;
	type NativeRewardsSource = UnreleasedNativeVaultAccountId;
	type NativeCurrencyId = GetNativeCurrencyId;
	type StableCurrencyId = GetStableCurrencyId;
	type LiquidCurrencyId = GetLiquidCurrencyId;
	type AccumulatePeriod = AccumulatePeriod;
	type UpdateOrigin = EnsureRootOrThreeFourthsGeneralCouncil;
	type CDPTreasury = CdpTreasury;
	type Currency = Currencies;
	type DEX = Dex;
	type EmergencyShutdown = EmergencyShutdown;
	type PalletId = IncentivesPalletId;
	type WeightInfo = weights::module_incentives::WeightInfo<Runtime>;
}

parameter_types! {
	pub CreateClassDeposit: Balance = 20 * dollar(KAR);
	pub CreateTokenDeposit: Balance = 2 * dollar(KAR);
	pub MaxAttributesBytes: u32 = 2048;
}

impl module_nft::Config for Runtime {
	type Event = Event;
	type Currency = Balances;
	type CreateClassDeposit = CreateClassDeposit;
	type CreateTokenDeposit = CreateTokenDeposit;
	type DataDepositPerByte = DataDepositPerByte;
	type PalletId = NftPalletId;
	type MaxAttributesBytes = MaxAttributesBytes;
	type WeightInfo = weights::module_nft::WeightInfo<Runtime>;
}

parameter_types! {
	pub MaxClassMetadata: u32 = 1024;
	pub MaxTokenMetadata: u32 = 1024;
}

impl orml_nft::Config for Runtime {
	type ClassId = u32;
	type TokenId = u64;
	type ClassData = module_nft::ClassData<Balance>;
	type TokenData = module_nft::TokenData<Balance>;
	type MaxClassMetadata = MaxClassMetadata;
	type MaxTokenMetadata = MaxTokenMetadata;
}

parameter_types! {
	// One storage item; key size 32, value size 8; .
	pub ProxyDepositBase: Balance = deposit(1, 8);
	// Additional storage item size of 33 bytes.
	pub ProxyDepositFactor: Balance = deposit(0, 33);
	pub const MaxProxies: u16 = 32;
	pub AnnouncementDepositBase: Balance = deposit(1, 8);
	pub AnnouncementDepositFactor: Balance = deposit(0, 66);
	pub const MaxPending: u16 = 32;
}

impl InstanceFilter<Call> for ProxyType {
	fn filter(&self, c: &Call) -> bool {
		match self {
			// Always allowed Call::Utility no matter type.
			// Only transactions allowed by Proxy.filter can be executed,
			// otherwise `BadOrigin` will be returned in Call::Utility.
			_ if matches!(c, Call::Utility(..)) => true,
			ProxyType::Any => true,
			ProxyType::CancelProxy => matches!(c, Call::Proxy(pallet_proxy::Call::reject_announcement(..))),
			ProxyType::Governance => {
				matches!(
					c,
					Call::Authority(..)
						| Call::Democracy(..) | Call::GeneralCouncil(..)
						| Call::FinancialCouncil(..)
						| Call::HomaCouncil(..) | Call::TechnicalCommittee(..)
						| Call::Treasury(..) | Call::Bounties(..)
						| Call::Tips(..)
				)
			}
			ProxyType::Auction => {
				matches!(c, Call::Auction(orml_auction::Call::bid(..)))
			}
			ProxyType::Swap => {
				matches!(
					c,
					Call::Dex(module_dex::Call::swap_with_exact_supply(..))
						| Call::Dex(module_dex::Call::swap_with_exact_target(..))
				)
			}
			ProxyType::Loan => {
				matches!(
					c,
					Call::Honzon(module_honzon::Call::adjust_loan(..))
						| Call::Honzon(module_honzon::Call::close_loan_has_debit_by_dex(..))
				)
			}
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(x, y) if x == y => true,
			(ProxyType::Any, _) => true,
			(_, ProxyType::Any) => false,
			_ => false,
		}
	}
}

impl pallet_proxy::Config for Runtime {
	type Event = Event;
	type Call = Call;
	type Currency = Balances;
	type ProxyType = ProxyType;
	type ProxyDepositBase = ProxyDepositBase;
	type ProxyDepositFactor = ProxyDepositFactor;
	type MaxProxies = MaxProxies;
	type WeightInfo = ();
	type MaxPending = MaxPending;
	type CallHasher = BlakeTwo256;
	type AnnouncementDepositBase = AnnouncementDepositBase;
	type AnnouncementDepositFactor = AnnouncementDepositFactor;
}

parameter_types! {
	pub const ChainId: u64 = 686;
	pub const NewContractExtraBytes: u32 = 10_000;
	pub StorageDepositPerByte: Balance = deposit(0, 1);
	// https://eips.ethereum.org/EIPS/eip-170
	pub const MaxCodeSize: u32 = 0x6000;
	pub NetworkContractSource: H160 = H160::from_low_u64_be(0);
	pub DeveloperDeposit: Balance = 100 * dollar(KAR);
	pub DeploymentFee: Balance = 10000 * dollar(KAR);
}

pub type MultiCurrencyPrecompile = runtime_common::MultiCurrencyPrecompile<
	AccountId,
	EvmAddressMapping<Runtime>,
	EvmCurrencyIdMapping<Runtime>,
	Currencies,
>;

pub type NFTPrecompile =
	runtime_common::NFTPrecompile<AccountId, EvmAddressMapping<Runtime>, EvmCurrencyIdMapping<Runtime>, NFT>;
pub type StateRentPrecompile =
	runtime_common::StateRentPrecompile<AccountId, EvmAddressMapping<Runtime>, EvmCurrencyIdMapping<Runtime>, EVM>;
pub type OraclePrecompile = runtime_common::OraclePrecompile<
	AccountId,
	EvmAddressMapping<Runtime>,
	EvmCurrencyIdMapping<Runtime>,
	module_prices::RealTimePriceProvider<Runtime>,
>;
pub type ScheduleCallPrecompile = runtime_common::ScheduleCallPrecompile<
	AccountId,
	EvmAddressMapping<Runtime>,
	EvmCurrencyIdMapping<Runtime>,
	Scheduler,
	module_transaction_payment::ChargeTransactionPayment<Runtime>,
	Call,
	Origin,
	OriginCaller,
	Runtime,
>;
pub type DexPrecompile =
	runtime_common::DexPrecompile<AccountId, EvmAddressMapping<Runtime>, EvmCurrencyIdMapping<Runtime>, Dex>;

impl module_evm::Config for Runtime {
	type AddressMapping = EvmAddressMapping<Runtime>;
	type Currency = Balances;
	type TransferAll = Currencies;
	type NewContractExtraBytes = NewContractExtraBytes;
	type StorageDepositPerByte = StorageDepositPerByte;
	type MaxCodeSize = MaxCodeSize;
	type Event = Event;
	type Precompiles = runtime_common::AllPrecompiles<
		SystemContractsFilter,
		MultiCurrencyPrecompile,
		NFTPrecompile,
		StateRentPrecompile,
		OraclePrecompile,
		ScheduleCallPrecompile,
		DexPrecompile,
	>;
	type ChainId = ChainId;
	type GasToWeight = GasToWeight;
	type ChargeTransactionPayment = module_transaction_payment::ChargeTransactionPayment<Runtime>;
	type NetworkContractOrigin = EnsureRootOrTwoThirdsTechnicalCommittee;
	type NetworkContractSource = NetworkContractSource;
	type DeveloperDeposit = DeveloperDeposit;
	type DeploymentFee = DeploymentFee;
	type TreasuryAccount = KaruraTreasuryAccount;
	type FreeDeploymentOrigin = EnsureRootOrHalfGeneralCouncil;
	type WeightInfo = weights::module_evm::WeightInfo<Runtime>;
}

impl module_evm_bridge::Config for Runtime {
	type EVM = EVM;
}

impl module_session_manager::Config for Runtime {
	type Event = Event;
	type ValidatorSet = Session;
	type WeightInfo = weights::module_session_manager::WeightInfo<Runtime>;
}

parameter_types! {
	pub ReservedXcmpWeight: Weight = RuntimeBlockWeights::get().max_block / 4;
	pub ReservedDmpWeight: Weight = RuntimeBlockWeights::get().max_block / 4;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type Event = Event;
	type OnValidationData = ();
	type SelfParaId = ParachainInfo;
	type DmpMessageHandler = DmpQueue;
	type ReservedDmpWeight = ReservedDmpWeight;
	type OutboundXcmpMessageSource = XcmpQueue;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ReservedXcmpWeight;
}

impl parachain_info::Config for Runtime {}

impl cumulus_pallet_aura_ext::Config for Runtime {}

parameter_types! {
	pub const KsmLocation: MultiLocation = MultiLocation::X1(Parent);
	pub const RelayNetwork: NetworkId = NetworkId::Kusama;
	pub RelayChainOrigin: Origin = cumulus_pallet_xcm::Origin::Relay.into();
	pub Ancestry: MultiLocation = Parachain(ParachainInfo::parachain_id().into()).into();
}

/// Type for specifying how a `MultiLocation` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the default `AccountId`.
	ParentIsDefault<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
);

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToCallOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, Origin>,
	// Native converter for Relay-chain (Parent) location; will converts to a `Relay` origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, Origin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, Origin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `Origin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, Origin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<Origin>,
);

parameter_types! {
	// One XCM operation is 200_000_000 weight, cross-chain transfer ~= 2x of transfer.
	pub const UnitWeightCost: Weight = 200_000_000;
	pub KsmPerSecond: (MultiLocation, u128) = (X1(Parent), ksm_per_second());
}

pub type Barrier = (TakeWeightCredit, AllowTopLevelPaidExecutionFrom<All<MultiLocation>>);

pub struct ToTreasury;
impl TakeRevenue for ToTreasury {
	fn take_revenue(revenue: MultiAsset) {
		if let MultiAsset::ConcreteFungible { id, amount } = revenue {
			if let Some(currency_id) = CurrencyIdConvert::convert(id) {
				// ensure KaruraTreasuryAccount have ed for all of the cross-chain asset.
				// Ignore the result.
				let _ = Currencies::deposit(currency_id, &KaruraTreasuryAccount::get(), amount);
			}
		}
	}
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type Call = Call;
	type XcmSender = XcmRouter;
	// How to withdraw and deposit an asset.
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = XcmOriginToCallOrigin;
	type IsReserve = MultiNativeAsset;
	// Teleporting is disabled.
	type IsTeleporter = ();
	type LocationInverter = LocationInverter<Ancestry>;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call>;
	// Only receiving KSM is handled, and all fees must be paid in KSM.
	type Trader = FixedRateOfConcreteFungible<KsmPerSecond, ToTreasury>;
	type ResponseHandler = (); // Don't handle responses for now.
}

parameter_types! {
	pub MaxDownwardMessageWeight: Weight = RuntimeBlockWeights::get().max_block / 10;
}

pub type LocalOriginToLocation = SignedToAccountId32<Origin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = (
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
);

impl pallet_xcm::Config for Runtime {
	type Event = Event;
	type SendXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	type XcmExecuteFilter = All<(MultiLocation, Xcm<Call>)>;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = ();
	type XcmReserveTransferFilter = All<(MultiLocation, Vec<MultiAsset>)>;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call>;
}

impl cumulus_pallet_xcm::Config for Runtime {
	type Event = Event;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type Event = Event;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ChannelInfo = ParachainSystem;
}

impl cumulus_pallet_dmp_queue::Config for Runtime {
	type Event = Event;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
}

pub fn create_x2_parachain_multilocation(index: u16) -> MultiLocation {
	MultiLocation::X2(
		Junction::Parent,
		Junction::AccountId32 {
			network: RelayNetwork::get(),
			id: Utility::derivative_account_id(ParachainInfo::get().into_account(), index).into(),
		},
	)
}

parameter_types! {
	pub const KSMCurrencyId: CurrencyId = CurrencyId::Token(TokenSymbol::KSM);
	pub const LKSMCurrencyId: CurrencyId = CurrencyId::Token(TokenSymbol::LKSM);
	pub MinimumMintThreshold: Balance = 10 * millicent(KSM);
	pub RelaychainSovereignSubAccount: MultiLocation = create_x2_parachain_multilocation(RELAYCHAIN_SUB_ACCOUNT_ID);
	pub MaxRewardPerEra: Permill = Permill::from_rational(411u32, 1_000_000u32); // 15% / 365 = 0.0004109
	pub MintFee: Balance = millicent(KSM);
	pub DefaultExchangeRate: ExchangeRate = ExchangeRate::saturating_from_rational(1, 10);
}
impl module_homa_lite::Config for Runtime {
	type Event = Event;
	type WeightInfo = weights::module_homa_lite::WeightInfo<Runtime>;
	type Currency = Currencies;
	type StakingCurrencyId = KSMCurrencyId;
	type LiquidCurrencyId = LKSMCurrencyId;
	type GovernanceOrigin = EnsureRootOrHalfGeneralCouncil;
	type MinimumMintThreshold = MinimumMintThreshold;
	type XcmTransfer = XTokens;
	type SovereignSubAccountLocation = RelaychainSovereignSubAccount;
	type DefaultExchangeRate = DefaultExchangeRate;
	type MaxRewardPerEra = MaxRewardPerEra;
	type MintFee = MintFee;
}

pub type LocalAssetTransactor = MultiCurrencyAdapter<
	Currencies,
	UnknownTokens,
	IsNativeConcrete<CurrencyId, CurrencyIdConvert>,
	AccountId,
	LocationToAccountId,
	CurrencyId,
	CurrencyIdConvert,
>;

//TODO: use token registry currency type encoding
fn native_currency_location(id: CurrencyId) -> MultiLocation {
	X3(Parent, Parachain(ParachainInfo::get().into()), GeneralKey(id.encode()))
}

pub struct CurrencyIdConvert;
impl Convert<CurrencyId, Option<MultiLocation>> for CurrencyIdConvert {
	fn convert(id: CurrencyId) -> Option<MultiLocation> {
		use CurrencyId::Token;
		use TokenSymbol::*;
		match id {
			Token(KSM) => Some(X1(Parent)),
			Token(KAR) | Token(KUSD) | Token(LKSM) | Token(RENBTC) => Some(native_currency_location(id)),
			_ => None,
		}
	}
}
impl Convert<MultiLocation, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(location: MultiLocation) -> Option<CurrencyId> {
		use CurrencyId::Token;
		use TokenSymbol::*;
		match location {
			X1(Parent) => Some(Token(KSM)),
			X3(Parent, Parachain(id), GeneralKey(key)) if ParaId::from(id) == ParachainInfo::get() => {
				// decode the general key
				if let Ok(currency_id) = CurrencyId::decode(&mut &key[..]) {
					// check `currency_id` is cross-chain asset
					match currency_id {
						Token(KAR) | Token(KUSD) | Token(LKSM) | Token(RENBTC) => Some(currency_id),
						_ => None,
					}
				} else {
					None
				}
			}
			_ => None,
		}
	}
}
impl Convert<MultiAsset, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(asset: MultiAsset) -> Option<CurrencyId> {
		if let MultiAsset::ConcreteFungible { id, amount: _ } = asset {
			Self::convert(id)
		} else {
			None
		}
	}
}

parameter_types! {
	pub SelfLocation: MultiLocation = X2(Parent, Parachain(ParachainInfo::get().into()));
}

pub struct AccountIdToMultiLocation;
impl Convert<AccountId, MultiLocation> for AccountIdToMultiLocation {
	fn convert(account: AccountId) -> MultiLocation {
		X1(AccountId32 {
			network: NetworkId::Any,
			id: account.into(),
		})
	}
}

parameter_types! {
	pub const BaseXcmWeight: Weight = 100_000_000;
}

impl orml_xtokens::Config for Runtime {
	type Event = Event;
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type CurrencyIdConvert = CurrencyIdConvert;
	type AccountIdToMultiLocation = AccountIdToMultiLocation;
	type SelfLocation = SelfLocation;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call>;
	type BaseXcmWeight = BaseXcmWeight;
}

impl orml_unknown_tokens::Config for Runtime {
	type Event = Event;
}

impl orml_xcm::Config for Runtime {
	type Event = Event;
	type SovereignOrigin = EnsureRootOrHalfGeneralCouncil;
}

#[allow(clippy::large_enum_variant)]
construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = primitives::Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		// Core & Utility
		System: frame_system::{Pallet, Call, Storage, Config, Event<T>} = 0,
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent} = 1,
		Scheduler: pallet_scheduler::{Pallet, Call, Storage, Event<T>} = 2,
		Utility: pallet_utility::{Pallet, Call, Event} = 3,
		Multisig: pallet_multisig::{Pallet, Call, Storage, Event<T>} = 4,
		Proxy: pallet_proxy::{Pallet, Call, Storage, Event<T>} = 5,

		// Tokens & Related
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 10,
		Tokens: orml_tokens::{Pallet, Storage, Event<T>, Config<T>} = 11,
		Currencies: module_currencies::{Pallet, Call, Event<T>} = 12,
		Vesting: orml_vesting::{Pallet, Storage, Call, Event<T>, Config<T>} = 13,
		TransactionPayment: module_transaction_payment::{Pallet, Call, Storage} = 14,

		// Treasury
		Treasury: pallet_treasury::{Pallet, Call, Storage, Config, Event<T>} = 20,
		Bounties: pallet_bounties::{Pallet, Call, Storage, Event<T>} = 21,
		Tips: pallet_tips::{Pallet, Call, Storage, Event<T>} = 22,

		// Parachain
		ParachainSystem: cumulus_pallet_parachain_system::{Pallet, Call, Storage, Inherent, Config, Event<T>} = 30,
		ParachainInfo: parachain_info::{Pallet, Storage, Config} = 31,

		// Collator. The order of the 4 below are important and shall not change.
		Authorship: pallet_authorship::{Pallet, Call, Storage} = 40,
		CollatorSelection: module_collator_selection::{Pallet, Call, Storage, Event<T>, Config<T>} = 41,
		Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>} = 42,
		Aura: pallet_aura::{Pallet, Storage, Config<T>} = 43,
		AuraExt: cumulus_pallet_aura_ext::{Pallet, Storage, Config} = 44,
		SessionManager: module_session_manager::{Pallet, Call, Storage, Event<T>, Config<T>} = 45,

		// XCM
		XcmpQueue: cumulus_pallet_xcmp_queue::{Pallet, Call, Storage, Event<T>} = 50,
		PolkadotXcm: pallet_xcm::{Pallet, Call, Event<T>, Origin} = 51,
		CumulusXcm: cumulus_pallet_xcm::{Pallet, Event<T>, Origin} = 52,
		DmpQueue: cumulus_pallet_dmp_queue::{Pallet, Call, Storage, Event<T>} = 53,
		XTokens: orml_xtokens::{Pallet, Storage, Call, Event<T>} = 54,
		UnknownTokens: orml_unknown_tokens::{Pallet, Storage, Event} = 55,
		OrmlXcm: orml_xcm::{Pallet, Call, Event<T>} = 56,

		// Governance
		Authority: orml_authority::{Pallet, Call, Storage, Event<T>, Origin<T>} = 60,
		GeneralCouncil: pallet_collective::<Instance1>::{Pallet, Call, Storage, Origin<T>, Event<T>, Config<T>} = 61,
		GeneralCouncilMembership: pallet_membership::<Instance1>::{Pallet, Call, Storage, Event<T>, Config<T>} = 62,
		FinancialCouncil: pallet_collective::<Instance2>::{Pallet, Call, Storage, Origin<T>, Event<T>, Config<T>} = 63,
		FinancialCouncilMembership: pallet_membership::<Instance2>::{Pallet, Call, Storage, Event<T>, Config<T>} = 64,
		HomaCouncil: pallet_collective::<Instance3>::{Pallet, Call, Storage, Origin<T>, Event<T>, Config<T>} = 65,
		HomaCouncilMembership: pallet_membership::<Instance3>::{Pallet, Call, Storage, Event<T>, Config<T>} = 66,
		TechnicalCommittee: pallet_collective::<Instance4>::{Pallet, Call, Storage, Origin<T>, Event<T>, Config<T>} = 67,
		TechnicalCommitteeMembership: pallet_membership::<Instance4>::{Pallet, Call, Storage, Event<T>, Config<T>} = 68,
		Democracy: pallet_democracy::{Pallet, Call, Storage, Config<T>, Event<T>} = 69,

		// Oracle
		//
		// NOTE: OperatorMembership must be placed after Oracle or else will have race condition on initialization
		AcalaOracle: orml_oracle::<Instance1>::{Pallet, Storage, Call, Event<T>} = 70,
		OperatorMembershipAcala: pallet_membership::<Instance5>::{Pallet, Call, Storage, Event<T>, Config<T>} = 71,

		// ORML Core
		Auction: orml_auction::{Pallet, Storage, Call, Event<T>} = 80,
		Rewards: orml_rewards::{Pallet, Storage, Call} = 81,
		OrmlNFT: orml_nft::{Pallet, Storage, Config<T>} = 82,

		// Karura Core
		Prices: module_prices::{Pallet, Storage, Call, Event<T>} = 90,
		Dex: module_dex::{Pallet, Storage, Call, Event<T>, Config<T>} = 91,

		// Honzon
		AuctionManager: module_auction_manager::{Pallet, Storage, Call, Event<T>, ValidateUnsigned} = 100,
		Loans: module_loans::{Pallet, Storage, Call, Event<T>} = 101,
		Honzon: module_honzon::{Pallet, Storage, Call, Event<T>} = 102,
		CdpTreasury: module_cdp_treasury::{Pallet, Storage, Call, Config, Event<T>} = 103,
		CdpEngine: module_cdp_engine::{Pallet, Storage, Call, Event<T>, Config, ValidateUnsigned} = 104,
		EmergencyShutdown: module_emergency_shutdown::{Pallet, Storage, Call, Event<T>} = 105,

		// Homa
		// Homa: module_homa::{Pallet, Call} = 110,
		// NomineesElection: module_nominees_election::{Pallet, Call, Storage, Event<T>} = 111,
		// StakingPool: module_staking_pool::{Pallet, Call, Storage, Event<T>, Config} = 112,
		// PolkadotBridge: module_polkadot_bridge::{Pallet, Call, Storage} = 113,
		// HomaValidatorListModule: module_homa_validator_list::{Pallet, Call, Storage, Event<T>} = 114,
		HomaLite: module_homa_lite::{Pallet, Call, Storage, Event<T>} = 115,

		// Karura Other
		Incentives: module_incentives::{Pallet, Storage, Call, Event<T>} = 120,
		NFT: module_nft::{Pallet, Call, Event<T>} = 121,

		// Smart contracts
		EVM: module_evm::{Pallet, Config<T>, Call, Storage, Event<T>} = 130,
		EVMBridge: module_evm_bridge::{Pallet} = 131,
		EvmAccounts: module_evm_accounts::{Pallet, Call, Storage, Event<T>} = 132,
		EvmManager: module_evm_manager::{Pallet, Storage} = 133,

		// Temporary
		Sudo: pallet_sudo::{Pallet, Call, Config<T>, Storage, Event<T>} = 255,
	}
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, AccountIndex>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	module_transaction_payment::ChargeTransactionPayment<Runtime>,
	module_evm::SetEvmOrigin<Runtime>,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, Call, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<Call, SignedExtra>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, Call, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive =
	frame_executive::Executive<Runtime, Block, frame_system::ChainContext<Runtime>, Runtime, AllPallets, ()>;

#[cfg(not(feature = "disable-runtime-api"))]
impl_runtime_apis! {
	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block)
		}

		fn initialize_block(header: &<Block as BlockT>::Header) {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			Runtime::metadata().into()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			Aura::authorities()
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
			SessionKeys::generate(seed)
		}

		fn decode_session_keys(
			encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
			System::account_nonce(account)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<
		Block,
		Balance,
	> for Runtime {
		fn query_info(uxt: <Block as BlockT>::Extrinsic, len: u32) -> RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_info(uxt, len)
		}

		fn query_fee_details(uxt: <Block as BlockT>::Extrinsic, len: u32) -> pallet_transaction_payment_rpc_runtime_api::FeeDetails<Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
	}

	impl orml_oracle_rpc_runtime_api::OracleApi<
		Block,
		DataProviderId,
		CurrencyId,
		TimeStampedPrice,
	> for Runtime {
		fn get_value(provider_id: DataProviderId ,key: CurrencyId) -> Option<TimeStampedPrice> {
			match provider_id {
				DataProviderId::Acala => AcalaOracle::get_no_op(&key),
				DataProviderId::Aggregated => <AggregatedDataProvider as DataProviderExtended<_, _>>::get_no_op(&key)
			}
		}

		fn get_all_values(provider_id: DataProviderId) -> Vec<(CurrencyId, Option<TimeStampedPrice>)> {
			match provider_id {
				DataProviderId::Acala => AcalaOracle::get_all_values(),
				DataProviderId::Aggregated => <AggregatedDataProvider as DataProviderExtended<_, _>>::get_all_values()
			}
		}
	}

	impl module_staking_pool_rpc_runtime_api::StakingPoolApi<
		Block,
		AccountId,
		Balance,
	> for Runtime {
		fn get_available_unbonded(_account: AccountId) -> module_staking_pool_rpc_runtime_api::BalanceInfo<Balance> {
			module_staking_pool_rpc_runtime_api::BalanceInfo {
				amount: Zero::zero()
			}
		}

		fn get_liquid_staking_exchange_rate() -> ExchangeRate {
			ExchangeRate::zero()
		}
	}

	impl module_evm_rpc_runtime_api::EVMRuntimeRPCApi<Block, Balance> for Runtime {
		fn call(
			from: H160,
			to: H160,
			data: Vec<u8>,
			value: Balance,
			gas_limit: u64,
			storage_limit: u32,
			estimate: bool,
		) -> Result<CallInfo, sp_runtime::DispatchError> {
			let config = if estimate {
				let mut config = <Runtime as module_evm::Config>::config().clone();
				config.estimate = true;
				Some(config)
			} else {
				None
			};

			module_evm::Runner::<Runtime>::call(
				from,
				from,
				to,
				data,
				value,
				gas_limit,
				storage_limit,
				config.as_ref().unwrap_or(<Runtime as module_evm::Config>::config()),
			)
		}

		fn create(
			from: H160,
			data: Vec<u8>,
			value: Balance,
			gas_limit: u64,
			storage_limit: u32,
			estimate: bool,
		) -> Result<CreateInfo, sp_runtime::DispatchError> {
			let config = if estimate {
				let mut config = <Runtime as module_evm::Config>::config().clone();
				config.estimate = true;
				Some(config)
			} else {
				None
			};

			module_evm::Runner::<Runtime>::create(
				from,
				data,
				value,
				gas_limit,
				storage_limit,
				config.as_ref().unwrap_or(<Runtime as module_evm::Config>::config()),
			)
		}

		fn get_estimate_resources_request(extrinsic: Vec<u8>) -> Result<EstimateResourcesRequest, sp_runtime::DispatchError> {
			let utx = UncheckedExtrinsic::decode(&mut &*extrinsic)
				.map_err(|_| sp_runtime::DispatchError::Other("Invalid parameter extrinsic, decode failed"))?;

			let request = match utx.function {
				Call::EVM(module_evm::Call::call(to, data, value, gas_limit, storage_limit)) => {
					Some(EstimateResourcesRequest {
						from: None,
						to: Some(to),
						gas_limit: Some(gas_limit),
						storage_limit: Some(storage_limit),
						value: Some(value),
						data: Some(data),
					})
				}
				Call::EVM(module_evm::Call::create(data, value, gas_limit, storage_limit)) => {
					Some(EstimateResourcesRequest {
						from: None,
						to: None,
						gas_limit: Some(gas_limit),
						storage_limit: Some(storage_limit),
						value: Some(value),
						data: Some(data),
					})
				}
				_ => None,
			};

			request.ok_or(sp_runtime::DispatchError::Other("Invalid parameter extrinsic, not evm Call"))
		}
	}

	impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
		fn collect_collation_info() -> cumulus_primitives_core::CollationInfo {
			ParachainSystem::collect_collation_info()
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade() -> Result<(Weight, Weight), sp_runtime::RuntimeString> {
			let weight = Executive::try_runtime_upgrade()?;
			Ok((weight, RuntimeBlockWeights::get().max_block))
		}
	}

	// benchmarks for acala modules
	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{Benchmarking, BenchmarkBatch, add_benchmark, TrackedStorageKey};
			use orml_benchmarking::{add_benchmark as orml_add_benchmark};

			use module_nft::benchmarking::Pallet as NftBench;
			use module_homa_lite::benchmarking::Pallet as HomaLiteBench;

			let whitelist: Vec<TrackedStorageKey> = vec![
				// Block Number
				// frame_system::Number::<Runtime>::hashed_key().to_vec(),
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac").to_vec().into(),
				// Total Issuance
				hex_literal::hex!("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80").to_vec().into(),
				// Execution Phase
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a").to_vec().into(),
				// Event Count
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850").to_vec().into(),
				// System Events
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7").to_vec().into(),
				// Caller 0 Account
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da946c154ffd9992e395af90b5b13cc6f295c77033fce8a9045824a6690bbf99c6db269502f0a8d1d2a008542d5690a0749").to_vec().into(),
				// Treasury Account
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da95ecffd7b6c0f78751baa9d281e0bfa3a6d6f646c70792f74727372790000000000000000000000000000000000000000").to_vec().into(),
			];
			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);

			add_benchmark!(params, batches, module_nft, NftBench::<Runtime>);
			add_benchmark!(params, batches, module_homa_lite, HomaLiteBench::<Runtime>);
			orml_add_benchmark!(params, batches, module_dex, benchmarking::dex);
			orml_add_benchmark!(params, batches, module_auction_manager, benchmarking::auction_manager);
			orml_add_benchmark!(params, batches, module_cdp_engine, benchmarking::cdp_engine);
			orml_add_benchmark!(params, batches, module_emergency_shutdown, benchmarking::emergency_shutdown);
			orml_add_benchmark!(params, batches, module_evm, benchmarking::evm);
			orml_add_benchmark!(params, batches, module_honzon, benchmarking::honzon);
			orml_add_benchmark!(params, batches, module_cdp_treasury, benchmarking::cdp_treasury);
			orml_add_benchmark!(params, batches, module_collator_selection, benchmarking::collator_selection);
			// orml_add_benchmark!(params, batches, module_nominees_election, benchmarking::nominees_election);
			orml_add_benchmark!(params, batches, module_transaction_payment, benchmarking::transaction_payment);
			orml_add_benchmark!(params, batches, module_incentives, benchmarking::incentives);
			orml_add_benchmark!(params, batches, module_prices, benchmarking::prices);
			orml_add_benchmark!(params, batches, module_evm_accounts, benchmarking::evm_accounts);
			// orml_add_benchmark!(params, batches, module_homa, benchmarking::homa);
			orml_add_benchmark!(params, batches, module_currencies, benchmarking::currencies);
			orml_add_benchmark!(params, batches, module_session_manager, benchmarking::session_manager);

			orml_add_benchmark!(params, batches, orml_tokens, benchmarking::tokens);
			orml_add_benchmark!(params, batches, orml_vesting, benchmarking::vesting);
			orml_add_benchmark!(params, batches, orml_auction, benchmarking::auction);

			orml_add_benchmark!(params, batches, orml_authority, benchmarking::authority);
			orml_add_benchmark!(params, batches, orml_oracle, benchmarking::oracle);

			if batches.is_empty() { return Err("Benchmark not found for this module.".into()) }
			Ok(batches)
		}
	}
}

struct CheckInherents;

impl cumulus_pallet_parachain_system::CheckInherents<Block> for CheckInherents {
	fn check_inherents(
		block: &Block,
		relay_state_proof: &cumulus_pallet_parachain_system::RelayChainStateProof,
	) -> sp_inherents::CheckInherentsResult {
		let relay_chain_slot = relay_state_proof
			.read_slot()
			.expect("Could not read the relay chain slot from the proof");

		let inherent_data = cumulus_primitives_timestamp::InherentDataProvider::from_relay_chain_slot_and_duration(
			relay_chain_slot,
			sp_std::time::Duration::from_secs(6),
		)
		.create_inherent_data()
		.expect("Could not create the timestamp inherent data");

		inherent_data.check_extrinsics(&block)
	}
}

cumulus_pallet_parachain_system::register_validate_block!(
	Runtime = Runtime,
	BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
	CheckInherents = CheckInherents,
);

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::weights::DispatchClass;
	use frame_system::offchain::CreateSignedTransaction;
	use sp_runtime::traits::Convert;

	fn run_with_system_weight<F>(w: Weight, mut assertions: F)
	where
		F: FnMut() -> (),
	{
		let mut t: sp_io::TestExternalities = frame_system::GenesisConfig::default()
			.build_storage::<Runtime>()
			.unwrap()
			.into();
		t.execute_with(|| {
			System::set_block_consumed_resources(w, 0);
			assertions()
		});
	}

	#[test]
	fn validate_transaction_submitter_bounds() {
		fn is_submit_signed_transaction<T>()
		where
			T: CreateSignedTransaction<Call>,
		{
		}

		is_submit_signed_transaction::<Runtime>();
	}

	#[test]
	fn multiplier_can_grow_from_zero() {
		let minimum_multiplier = MinimumMultiplier::get();
		let target =
			TargetBlockFullness::get() * RuntimeBlockWeights::get().get(DispatchClass::Normal).max_total.unwrap();
		// if the min is too small, then this will not change, and we are doomed forever.
		// the weight is 1/100th bigger than target.
		run_with_system_weight(target * 101 / 100, || {
			let next = SlowAdjustingFeeUpdate::<Runtime>::convert(minimum_multiplier);
			assert!(next > minimum_multiplier, "{:?} !>= {:?}", next, minimum_multiplier);
		})
	}

	#[test]
	fn ensure_can_create_contract() {
		// Ensure that the `ExistentialDeposit` for creating the contract >= account `ExistentialDeposit`.
		// Otherwise, the creation of the contract account will fail because it is less than
		// ExistentialDeposit.
		assert!(
			Balance::from(NewContractExtraBytes::get()) * StorageDepositPerByte::get()
				>= NativeTokenExistentialDeposit::get()
		);
	}

	#[test]
	fn ensure_can_kick_collator() {
		// Ensure that `required_point` > 0, collator can be kicked out normally.
		assert!(
			CollatorKickThreshold::get().mul_floor(
				(Period::get() * module_collator_selection::POINT_PER_BLOCK)
					.checked_div(MaxCandidates::get())
					.unwrap()
			) > 0
		);
	}
}
