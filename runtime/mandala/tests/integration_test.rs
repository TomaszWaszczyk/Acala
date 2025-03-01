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

#![cfg(test)]

use acala_service::chain_spec::evm_genesis;
use codec::Encode;
// use cumulus_primitives_core::{DownwardMessageHandler, InboundDownwardMessage,
// XcmpMessageHandler};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use frame_support::{
	assert_noop, assert_ok,
	traits::{schedule::DispatchTime, Currency, GenesisBuild, OnFinalize, OnInitialize, OriginTrait, ValidatorSet},
};
use frame_system::RawOrigin;
use mandala_runtime::{
	create_x2_parachain_multilocation, dollar, get_all_module_accounts, AcalaOracle, AccountId, AuctionManager,
	Authority, AuthoritysOriginId, Balance, Balances, BlockNumber, Call, CdpEngine, CdpTreasury, CreateClassDeposit,
	CreateTokenDeposit, Currencies, CurrencyId, CurrencyIdConvert, DataDepositPerByte, Dex, EVMBridge,
	EmergencyShutdown, EnabledTradingPairs, Event, EvmAccounts, EvmCurrencyIdMapping, ExistentialDeposits, Get,
	GetNativeCurrencyId, Loans, MultiLocation, NativeTokenExistentialDeposit, NetworkId, NftPalletId, Origin,
	OriginCaller, ParachainInfo, ParachainSystem, Perbill, Proxy, Runtime, Scheduler, Session, SessionManager,
	SevenDays, System, TokenSymbol, Tokens, TreasuryAccount, TreasuryPalletId, TreasuryReservePalletId, Vesting,
	XcmConfig, XcmExecutor, ACA, AUSD, DOT, EVM, LDOT, NFT, RENBTC,
};
use module_cdp_engine::LiquidationStrategy;
use module_evm_accounts::EvmAddressMapping;
use module_support::{
	mocks::MockAddressMapping, AddressMapping, CDPTreasury, CurrencyIdMapping, DEXManager, EVMBridge as EVMBridgeT,
	Price, Rate, Ratio, RiskManager, EVM as EVMTrait,
};
use orml_authority::DelayedOrigin;
use orml_traits::{Change, GetByKey, MultiCurrency};
use orml_vesting::VestingSchedule;
// use polkadot_parachain::primitives::Sibling;
pub use primitives::{evm::EvmAddress, DexShare, TradingPair};
use sp_core::{bytes::from_hex, H160};
use sp_io::hashing::keccak_256;
use sp_runtime::{
	traits::{AccountIdConversion, BadOrigin, Convert, Zero},
	DispatchError, DispatchResult, FixedPointNumber, MultiAddress,
};
use std::str::FromStr;
use xcm::{
	opaque::v0::prelude::{BuyExecution, DepositAsset},
	v0::{
		ExecuteXcm,
		Junction::{self, *},
		MultiAsset,
		MultiLocation::*,
		Outcome, Xcm,
	},
};

use primitives::currency::*;

const ORACLE1: [u8; 32] = [0u8; 32];
const ORACLE2: [u8; 32] = [1u8; 32];
const ORACLE3: [u8; 32] = [2u8; 32];

const ALICE: [u8; 32] = [4u8; 32];
const BOB: [u8; 32] = [5u8; 32];
const LPTOKEN: CurrencyId =
	CurrencyId::DexShare(DexShare::Token(TokenSymbol::AUSD), DexShare::Token(TokenSymbol::RENBTC));

fn run_to_block(n: u32) {
	while System::block_number() < n {
		Scheduler::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		Scheduler::on_initialize(System::block_number());
		Scheduler::on_initialize(System::block_number());
		Session::on_initialize(System::block_number());
		SessionManager::on_initialize(System::block_number());
	}
}

fn set_relaychain_block_number(number: BlockNumber) {
	ParachainSystem::on_initialize(number);

	let (relay_storage_root, proof) = RelayStateSproofBuilder::default().into_state_root_and_proof();

	assert_ok!(ParachainSystem::set_validation_data(
		Origin::none(),
		cumulus_primitives_parachain_inherent::ParachainInherentData {
			validation_data: cumulus_primitives_core::PersistedValidationData {
				parent_head: Default::default(),
				relay_parent_number: number,
				relay_parent_storage_root: relay_storage_root,
				max_pov_size: Default::default(),
			},
			relay_chain_state: proof,
			downward_messages: Default::default(),
			horizontal_messages: Default::default(),
		}
	));
}

pub fn erc20_address_0() -> EvmAddress {
	EvmAddress::from_str("0000000000000000000000000000000002000000").unwrap()
}

pub fn erc20_address_1() -> EvmAddress {
	EvmAddress::from_str("0000000000000000000000000000000002000001").unwrap()
}

pub fn lp_erc20() -> CurrencyId {
	CurrencyId::DexShare(DexShare::Erc20(erc20_address_0()), DexShare::Erc20(erc20_address_1()))
}

pub fn lp_erc20_evm_address() -> EvmAddress {
	EvmCurrencyIdMapping::<Runtime>::encode_evm_address(lp_erc20()).unwrap()
}

pub fn deploy_erc20_contracts() {
	let code = from_hex(include!("../../../modules/evm-bridge/src/erc20_demo_contract")).unwrap();
	assert_ok!(EVM::create_network_contract(
		Origin::root(),
		code.clone(),
		0,
		2100_000,
		100000
	));

	let event = Event::EVM(module_evm::Event::<Runtime>::Created(erc20_address_0()));
	assert_eq!(System::events().iter().last().unwrap().event, event);

	assert_ok!(EVM::deploy_free(Origin::root(), erc20_address_0()));

	assert_ok!(EVM::create_network_contract(Origin::root(), code, 0, 2100_000, 100000));

	let event = Event::EVM(module_evm::Event::<Runtime>::Created(erc20_address_1()));
	assert_eq!(System::events().iter().last().unwrap().event, event);

	assert_ok!(EVM::deploy_free(Origin::root(), erc20_address_1()));
}

pub struct ExtBuilder {
	balances: Vec<(AccountId, CurrencyId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { balances: vec![] }
	}
}

impl ExtBuilder {
	pub fn balances(mut self, balances: Vec<(AccountId, CurrencyId, Balance)>) -> Self {
		self.balances = balances;
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let evm_genesis_accounts = evm_genesis();

		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<Runtime>()
			.unwrap();

		let native_currency_id = GetNativeCurrencyId::get();
		let existential_deposit = NativeTokenExistentialDeposit::get();
		let initial_enabled_trading_pairs = EnabledTradingPairs::get();

		module_dex::GenesisConfig::<Runtime> {
			initial_enabled_trading_pairs: initial_enabled_trading_pairs,
			initial_listing_trading_pairs: Default::default(),
			initial_added_liquidity_pools: vec![],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		pallet_balances::GenesisConfig::<Runtime> {
			balances: self
				.balances
				.clone()
				.into_iter()
				.filter(|(_, currency_id, _)| *currency_id == native_currency_id)
				.map(|(account_id, _, initial_balance)| (account_id, initial_balance))
				.chain(
					get_all_module_accounts()
						.iter()
						.map(|x| (x.clone(), existential_deposit)),
				)
				.collect::<Vec<_>>(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		orml_tokens::GenesisConfig::<Runtime> {
			balances: self
				.balances
				.into_iter()
				.filter(|(_, currency_id, _)| *currency_id != native_currency_id)
				.collect::<Vec<_>>(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		pallet_membership::GenesisConfig::<Runtime, pallet_membership::Instance5> {
			members: vec![
				AccountId::from(ORACLE1),
				AccountId::from(ORACLE2),
				AccountId::from(ORACLE3),
			],
			phantom: Default::default(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		module_evm::GenesisConfig::<Runtime> {
			accounts: evm_genesis_accounts,
			treasury: Default::default(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		module_session_manager::GenesisConfig::<Runtime> { session_duration: 10 }
			.assimilate_storage(&mut t)
			.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}

fn set_oracle_price(prices: Vec<(CurrencyId, Price)>) -> DispatchResult {
	AcalaOracle::on_finalize(0);
	assert_ok!(AcalaOracle::feed_values(
		Origin::signed(AccountId::from(ORACLE1)),
		prices.clone(),
	));
	assert_ok!(AcalaOracle::feed_values(
		Origin::signed(AccountId::from(ORACLE2)),
		prices.clone(),
	));
	assert_ok!(AcalaOracle::feed_values(
		Origin::signed(AccountId::from(ORACLE3)),
		prices,
	));
	Ok(())
}

fn alice_key() -> secp256k1::SecretKey {
	secp256k1::SecretKey::parse(&keccak_256(b"Alice")).unwrap()
}

fn bob_key() -> secp256k1::SecretKey {
	secp256k1::SecretKey::parse(&keccak_256(b"Bob")).unwrap()
}

pub fn alice() -> AccountId {
	let address = EvmAccounts::eth_address(&alice_key());
	let mut data = [0u8; 32];
	data[0..4].copy_from_slice(b"evm:");
	data[4..24].copy_from_slice(&address[..]);
	AccountId::from(Into::<[u8; 32]>::into(data))
}

pub fn alice_evm_addr() -> EvmAddress {
	EvmAddress::from_str("1000000000000000000000000000000000000001").unwrap()
}

pub fn bob() -> AccountId {
	let address = EvmAccounts::eth_address(&bob_key());
	let mut data = [0u8; 32];
	data[0..4].copy_from_slice(b"evm:");
	data[4..24].copy_from_slice(&address[..]);
	AccountId::from(Into::<[u8; 32]>::into(data))
}

pub fn bob_evm_addr() -> EvmAddress {
	EvmAddress::from_str("1000000000000000000000000000000000000002").unwrap()
}

#[cfg(not(feature = "with-ethereum-compatibility"))]
fn deploy_contract(account: AccountId) -> Result<H160, DispatchError> {
	// pragma solidity ^0.5.0;
	//
	// contract Factory {
	//     Contract[] newContracts;
	//
	//     function createContract () public payable {
	//         Contract newContract = new Contract();
	//         newContracts.push(newContract);
	//     }
	// }
	//
	// contract Contract {}
	let contract = hex_literal::hex!("608060405234801561001057600080fd5b5061016f806100206000396000f3fe608060405260043610610041576000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063412a5a6d14610046575b600080fd5b61004e610050565b005b600061005a6100e2565b604051809103906000f080158015610076573d6000803e3d6000fd5b50905060008190806001815401808255809150509060018203906000526020600020016000909192909190916101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff1602179055505050565b6040516052806100f28339019056fe6080604052348015600f57600080fd5b50603580601d6000396000f3fe6080604052600080fdfea165627a7a7230582092dc1966a8880ddf11e067f9dd56a632c11a78a4afd4a9f05924d427367958cc0029a165627a7a723058202b2cc7384e11c452cdbf39b68dada2d5e10a632cc0174a354b8b8c83237e28a40029").to_vec();

	EVM::create(Origin::signed(account), contract, 0, 1000000000, 1000000000)
		.map_or_else(|e| Err(e.error), |_| Ok(()))?;

	if let Event::EVM(module_evm::Event::<Runtime>::Created(address)) = System::events().iter().last().unwrap().event {
		Ok(address)
	} else {
		Err("deploy_contract failed".into())
	}
}

#[test]
fn emergency_shutdown_and_cdp_treasury() {
	ExtBuilder::default()
		.balances(vec![
			(AccountId::from(ALICE), AUSD, 2_000_000u128),
			(AccountId::from(BOB), AUSD, 8_000_000u128),
			(AccountId::from(BOB), RENBTC, 1_000_000u128),
			(AccountId::from(BOB), DOT, 200_000_000u128),
			(AccountId::from(BOB), LDOT, 40_000_000u128),
		])
		.build()
		.execute_with(|| {
			assert_ok!(CdpTreasury::deposit_collateral(
				&AccountId::from(BOB),
				RENBTC,
				1_000_000
			));
			assert_ok!(CdpTreasury::deposit_collateral(&AccountId::from(BOB), DOT, 200_000_000));
			assert_ok!(CdpTreasury::deposit_collateral(&AccountId::from(BOB), LDOT, 40_000_000));
			assert_eq!(CdpTreasury::total_collaterals(RENBTC), 1_000_000);
			assert_eq!(CdpTreasury::total_collaterals(DOT), 200_000_000);
			assert_eq!(CdpTreasury::total_collaterals(LDOT), 40_000_000);

			assert_noop!(
				EmergencyShutdown::refund_collaterals(Origin::signed(AccountId::from(ALICE)), 1_000_000),
				module_emergency_shutdown::Error::<Runtime>::CanNotRefund,
			);
			assert_ok!(EmergencyShutdown::emergency_shutdown(Origin::root()));
			assert_ok!(EmergencyShutdown::open_collateral_refund(Origin::root()));
			assert_ok!(EmergencyShutdown::refund_collaterals(
				Origin::signed(AccountId::from(ALICE)),
				1_000_000
			));

			assert_eq!(CdpTreasury::total_collaterals(RENBTC), 900_000);
			assert_eq!(CdpTreasury::total_collaterals(DOT), 180_000_000);
			assert_eq!(CdpTreasury::total_collaterals(LDOT), 36_000_000);
			assert_eq!(Currencies::free_balance(AUSD, &AccountId::from(ALICE)), 1_000_000);
			assert_eq!(Currencies::free_balance(RENBTC, &AccountId::from(ALICE)), 100_000);
			assert_eq!(Currencies::free_balance(DOT, &AccountId::from(ALICE)), 20_000_000);
			assert_eq!(Currencies::free_balance(LDOT, &AccountId::from(ALICE)), 4_000_000);
		});
}

#[test]
fn liquidate_cdp() {
	ExtBuilder::default()
		.balances(vec![
			(AccountId::from(ALICE), RENBTC, 10 * dollar(RENBTC)),
			(AccountId::from(BOB), AUSD, 1_000_000 * dollar(AUSD)),
			(AccountId::from(BOB), RENBTC, 101 * dollar(RENBTC)),
		])
		.build()
		.execute_with(|| {
			assert_ok!(set_oracle_price(vec![(
				RENBTC,
				Price::saturating_from_rational(10000, 1)
			)])); // 10000 usd

			assert_ok!(Dex::add_liquidity(
				Origin::signed(AccountId::from(BOB)),
				RENBTC,
				AUSD,
				100 * dollar(RENBTC),
				1_000_000 * dollar(AUSD),
				0,
				false,
			));

			assert_ok!(CdpEngine::set_collateral_params(
				Origin::root(),
				RENBTC,
				Change::NewValue(Some(Rate::zero())),
				Change::NewValue(Some(Ratio::saturating_from_rational(200, 100))),
				Change::NewValue(Some(Rate::saturating_from_rational(20, 100))),
				Change::NewValue(Some(Ratio::saturating_from_rational(200, 100))),
				Change::NewValue(1_000_000 * dollar(AUSD)),
			));

			assert_ok!(CdpEngine::adjust_position(
				&AccountId::from(ALICE),
				RENBTC,
				(10 * dollar(RENBTC)) as i128,
				(500_000 * dollar(AUSD)) as i128,
			));

			assert_ok!(CdpEngine::adjust_position(
				&AccountId::from(BOB),
				RENBTC,
				dollar(RENBTC) as i128,
				(50_000 * dollar(AUSD)) as i128,
			));

			assert_eq!(
				Loans::positions(RENBTC, AccountId::from(ALICE)).debit,
				500_000 * dollar(AUSD)
			);
			assert_eq!(
				Loans::positions(RENBTC, AccountId::from(ALICE)).collateral,
				10 * dollar(RENBTC)
			);
			assert_eq!(
				Loans::positions(RENBTC, AccountId::from(BOB)).debit,
				50_000 * dollar(AUSD)
			);
			assert_eq!(
				Loans::positions(RENBTC, AccountId::from(BOB)).collateral,
				dollar(RENBTC)
			);
			assert_eq!(CdpTreasury::debit_pool(), 0);
			assert_eq!(AuctionManager::collateral_auctions(0), None);

			assert_ok!(CdpEngine::set_collateral_params(
				Origin::root(),
				RENBTC,
				Change::NoChange,
				Change::NewValue(Some(Ratio::saturating_from_rational(400, 100))),
				Change::NoChange,
				Change::NewValue(Some(Ratio::saturating_from_rational(400, 100))),
				Change::NoChange,
			));

			assert_ok!(CdpEngine::liquidate_unsafe_cdp(AccountId::from(ALICE), RENBTC));

			let liquidate_alice_xbtc_cdp_event = Event::CdpEngine(module_cdp_engine::Event::LiquidateUnsafeCDP(
				RENBTC,
				AccountId::from(ALICE),
				10 * dollar(RENBTC),
				50_000 * dollar(AUSD),
				LiquidationStrategy::Auction,
			));
			assert!(System::events()
				.iter()
				.any(|record| record.event == liquidate_alice_xbtc_cdp_event));

			assert_eq!(Loans::positions(RENBTC, AccountId::from(ALICE)).debit, 0);
			assert_eq!(Loans::positions(RENBTC, AccountId::from(ALICE)).collateral, 0);
			assert_eq!(AuctionManager::collateral_auctions(0).is_some(), true);
			assert_eq!(CdpTreasury::debit_pool(), 50_000 * dollar(AUSD));

			assert_ok!(CdpEngine::liquidate_unsafe_cdp(AccountId::from(BOB), RENBTC));

			let liquidate_bob_xbtc_cdp_event = Event::CdpEngine(module_cdp_engine::Event::LiquidateUnsafeCDP(
				RENBTC,
				AccountId::from(BOB),
				dollar(RENBTC),
				5_000 * dollar(AUSD),
				LiquidationStrategy::Exchange,
			));
			assert!(System::events()
				.iter()
				.any(|record| record.event == liquidate_bob_xbtc_cdp_event));

			assert_eq!(Loans::positions(RENBTC, AccountId::from(BOB)).debit, 0);
			assert_eq!(Loans::positions(RENBTC, AccountId::from(BOB)).collateral, 0);
			assert_eq!(CdpTreasury::debit_pool(), 55_000 * dollar(AUSD));
			assert!(CdpTreasury::surplus_pool() >= 5_000 * dollar(AUSD));
		});
}

#[test]
fn test_dex_module() {
	ExtBuilder::default()
		.balances(vec![
			(
				// NetworkContractSource
				MockAddressMapping::get_account_id(&H160::from_low_u64_be(0)),
				ACA,
				(1_000_000_000_000_000_000u128),
			),
			(
				// evm alice
				MockAddressMapping::get_account_id(&alice_evm_addr()),
				ACA,
				(1_000_000_000_000_000_000u128),
			),
			(AccountId::from(ALICE), AUSD, (1_000_000_000_000_000_000u128)),
			(AccountId::from(ALICE), RENBTC, (1_000_000_000_000_000_000u128)),
			(AccountId::from(BOB), AUSD, (1_000_000_000_000_000_000u128)),
			(AccountId::from(BOB), RENBTC, (1_000_000_000_000_000_000u128)),
		])
		.build()
		.execute_with(|| {
			deploy_erc20_contracts();
			assert_eq!(Dex::get_liquidity_pool(RENBTC, AUSD), (0, 0));
			assert_eq!(Currencies::total_issuance(LPTOKEN), 0);
			assert_eq!(Currencies::free_balance(LPTOKEN, &AccountId::from(ALICE)), 0);

			assert_noop!(
				Dex::add_liquidity(
					Origin::signed(AccountId::from(ALICE)),
					RENBTC,
					AUSD,
					0,
					10000000,
					0,
					false,
				),
				module_dex::Error::<Runtime>::InvalidLiquidityIncrement,
			);

			assert_ok!(Dex::add_liquidity(
				Origin::signed(AccountId::from(ALICE)),
				RENBTC,
				AUSD,
				10000,
				10000000,
				0,
				false,
			));

			let add_liquidity_event = Event::Dex(module_dex::Event::AddLiquidity(
				AccountId::from(ALICE),
				AUSD,
				10000000,
				RENBTC,
				10000,
				20000000,
			));
			assert!(System::events()
				.iter()
				.any(|record| record.event == add_liquidity_event));

			assert_eq!(Dex::get_liquidity_pool(RENBTC, AUSD), (10000, 10000000));
			assert_eq!(Currencies::total_issuance(LPTOKEN), 20000000);
			assert_eq!(Currencies::free_balance(LPTOKEN, &AccountId::from(ALICE)), 20000000);
			assert_ok!(Dex::add_liquidity(
				Origin::signed(AccountId::from(BOB)),
				RENBTC,
				AUSD,
				1,
				1000,
				0,
				false,
			));
			assert_eq!(Dex::get_liquidity_pool(RENBTC, AUSD), (10001, 10001000));
			assert_eq!(Currencies::total_issuance(LPTOKEN), 20002000);
			assert_eq!(Currencies::free_balance(LPTOKEN, &AccountId::from(BOB)), 2000);
			assert_noop!(
				Dex::add_liquidity(Origin::signed(AccountId::from(BOB)), RENBTC, AUSD, 1, 999, 0, false,),
				module_dex::Error::<Runtime>::InvalidLiquidityIncrement,
			);
			assert_eq!(Dex::get_liquidity_pool(RENBTC, AUSD), (10001, 10001000));
			assert_eq!(Currencies::total_issuance(LPTOKEN), 20002000);
			assert_eq!(Currencies::free_balance(LPTOKEN, &AccountId::from(BOB)), 2000);
			assert_ok!(Dex::add_liquidity(
				Origin::signed(AccountId::from(BOB)),
				RENBTC,
				AUSD,
				2,
				1000,
				0,
				false,
			));
			assert_eq!(Dex::get_liquidity_pool(RENBTC, AUSD), (10002, 10002000));
			assert_ok!(Dex::add_liquidity(
				Origin::signed(AccountId::from(BOB)),
				RENBTC,
				AUSD,
				1,
				1001,
				0,
				false,
			));
			assert_eq!(Dex::get_liquidity_pool(RENBTC, AUSD), (10003, 10003000));

			assert_eq!(Currencies::total_issuance(LPTOKEN), 20005998);

			// Erc20
			assert_ok!(EvmAccounts::claim_account(
				Origin::signed(AccountId::from(ALICE)),
				EvmAccounts::eth_address(&alice_key()),
				EvmAccounts::eth_sign(&alice_key(), &AccountId::from(ALICE).encode(), &[][..])
			));

			// CurrencyId::DexShare(Erc20, Erc20)
			assert_ok!(Dex::list_provisioning(
				Origin::root(),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
				10,
				100,
				100,
				1000,
				0,
			));

			<EVM as EVMTrait<AccountId>>::set_origin(MockAddressMapping::get_account_id(&alice_evm_addr()));
			assert_ok!(Dex::add_provision(
				Origin::signed(MockAddressMapping::get_account_id(&alice_evm_addr())),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
				10,
				100,
			));
			assert_eq!(
				Dex::get_liquidity_pool(
					CurrencyId::Erc20(erc20_address_0()),
					CurrencyId::Erc20(erc20_address_1())
				),
				(0, 0)
			);
			assert_eq!(
				Currencies::total_issuance(CurrencyId::DexShare(
					DexShare::Erc20(erc20_address_0()),
					DexShare::Erc20(erc20_address_1())
				)),
				0
			);
			assert_eq!(
				Currencies::free_balance(
					CurrencyId::DexShare(DexShare::Erc20(erc20_address_0()), DexShare::Erc20(erc20_address_1())),
					&AccountId::from(ALICE)
				),
				0
			);
			assert_eq!(
				Currencies::free_balance(
					CurrencyId::DexShare(DexShare::Erc20(erc20_address_0()), DexShare::Erc20(erc20_address_1())),
					&MockAddressMapping::get_account_id(&alice_evm_addr())
				),
				0
			);

			// CurrencyId::DexShare(Erc20, Erc20)
			<EVM as EVMTrait<AccountId>>::set_origin(EvmAddressMapping::<Runtime>::get_account_id(&alice_evm_addr()));

			assert_ok!(Dex::add_provision(
				Origin::signed(EvmAddressMapping::<Runtime>::get_account_id(&alice_evm_addr())),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
				100,
				1000,
			));
			assert_ok!(Dex::end_provisioning(
				Origin::signed(AccountId::from(BOB)),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
			));
			assert_eq!(
				Dex::get_liquidity_pool(
					CurrencyId::Erc20(erc20_address_0()),
					CurrencyId::Erc20(erc20_address_1())
				),
				(110, 1100)
			);

			assert_eq!(
				Currencies::total_issuance(CurrencyId::DexShare(
					DexShare::Erc20(erc20_address_0()),
					DexShare::Erc20(erc20_address_1())
				)),
				220
			);

			assert_ok!(Dex::claim_dex_share(
				Origin::signed(EvmAddressMapping::<Runtime>::get_account_id(&alice_evm_addr())),
				EvmAddressMapping::<Runtime>::get_account_id(&alice_evm_addr()),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
			));
			assert_eq!(
				Currencies::free_balance(
					CurrencyId::DexShare(DexShare::Erc20(erc20_address_0()), DexShare::Erc20(erc20_address_1())),
					&EvmAddressMapping::<Runtime>::get_account_id(&alice_evm_addr())
				),
				220
			);

			assert_ok!(Dex::remove_liquidity(
				Origin::signed(EvmAddressMapping::<Runtime>::get_account_id(&alice_evm_addr())),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
				1,
				0,
				0,
				false,
			));

			assert_eq!(
				Dex::get_liquidity_pool(
					CurrencyId::Erc20(erc20_address_0()),
					CurrencyId::Erc20(erc20_address_1())
				),
				(110, 1096)
			);

			assert_eq!(
				Currencies::total_issuance(CurrencyId::DexShare(
					DexShare::Erc20(erc20_address_0()),
					DexShare::Erc20(erc20_address_1())
				)),
				219
			);

			assert_eq!(
				Currencies::free_balance(
					CurrencyId::DexShare(DexShare::Erc20(erc20_address_0()), DexShare::Erc20(erc20_address_1())),
					&EvmAddressMapping::<Runtime>::get_account_id(&alice_evm_addr())
				),
				219
			);
		});
}

#[test]
fn test_honzon_module() {
	ExtBuilder::default()
		.balances(vec![(AccountId::from(ALICE), RENBTC, 1_000 * dollar(RENBTC))])
		.build()
		.execute_with(|| {
			assert_ok!(set_oracle_price(vec![(RENBTC, Price::saturating_from_rational(1, 1))]));

			assert_ok!(CdpEngine::set_collateral_params(
				Origin::root(),
				RENBTC,
				Change::NewValue(Some(Rate::saturating_from_rational(1, 100000))),
				Change::NewValue(Some(Ratio::saturating_from_rational(3, 2))),
				Change::NewValue(Some(Rate::saturating_from_rational(2, 10))),
				Change::NewValue(Some(Ratio::saturating_from_rational(9, 5))),
				Change::NewValue(10_000 * dollar(AUSD)),
			));
			assert_ok!(CdpEngine::adjust_position(
				&AccountId::from(ALICE),
				RENBTC,
				(100 * dollar(RENBTC)) as i128,
				(500 * dollar(AUSD)) as i128
			));
			assert_eq!(
				Currencies::free_balance(RENBTC, &AccountId::from(ALICE)),
				900 * dollar(RENBTC)
			);
			assert_eq!(
				Currencies::free_balance(AUSD, &AccountId::from(ALICE)),
				50 * dollar(AUSD)
			);
			assert_eq!(
				Loans::positions(RENBTC, AccountId::from(ALICE)).debit,
				500 * dollar(AUSD)
			);
			assert_eq!(
				Loans::positions(RENBTC, AccountId::from(ALICE)).collateral,
				100 * dollar(RENBTC)
			);
			assert_eq!(
				CdpEngine::liquidate(Origin::none(), RENBTC, MultiAddress::Id(AccountId::from(ALICE))).is_ok(),
				false
			);
			assert_ok!(CdpEngine::set_collateral_params(
				Origin::root(),
				RENBTC,
				Change::NoChange,
				Change::NewValue(Some(Ratio::saturating_from_rational(3, 1))),
				Change::NoChange,
				Change::NoChange,
				Change::NoChange,
			));
			assert_ok!(CdpEngine::liquidate(
				Origin::none(),
				RENBTC,
				MultiAddress::Id(AccountId::from(ALICE))
			));

			assert_eq!(
				Currencies::free_balance(RENBTC, &AccountId::from(ALICE)),
				900 * dollar(RENBTC)
			);
			assert_eq!(
				Currencies::free_balance(AUSD, &AccountId::from(ALICE)),
				50 * dollar(AUSD)
			);
			assert_eq!(Loans::positions(RENBTC, AccountId::from(ALICE)).debit, 0);
			assert_eq!(Loans::positions(RENBTC, AccountId::from(ALICE)).collateral, 0);
		});
}

#[test]
fn test_cdp_engine_module() {
	ExtBuilder::default()
		.balances(vec![
			(AccountId::from(ALICE), AUSD, 1_000 * dollar(AUSD)),
			(AccountId::from(ALICE), RENBTC, 1_000 * dollar(RENBTC)),
		])
		.build()
		.execute_with(|| {
			assert_ok!(CdpEngine::set_collateral_params(
				Origin::root(),
				RENBTC,
				Change::NewValue(Some(Rate::saturating_from_rational(1, 100000))),
				Change::NewValue(Some(Ratio::saturating_from_rational(3, 2))),
				Change::NewValue(Some(Rate::saturating_from_rational(2, 10))),
				Change::NewValue(Some(Ratio::saturating_from_rational(9, 5))),
				Change::NewValue(10_000 * dollar(AUSD)),
			));

			let new_collateral_params = CdpEngine::collateral_params(RENBTC);

			assert_eq!(
				new_collateral_params.interest_rate_per_sec,
				Some(Rate::saturating_from_rational(1, 100000))
			);
			assert_eq!(
				new_collateral_params.liquidation_ratio,
				Some(Ratio::saturating_from_rational(3, 2))
			);
			assert_eq!(
				new_collateral_params.liquidation_penalty,
				Some(Rate::saturating_from_rational(2, 10))
			);
			assert_eq!(
				new_collateral_params.required_collateral_ratio,
				Some(Ratio::saturating_from_rational(9, 5))
			);
			assert_eq!(new_collateral_params.maximum_total_debit_value, 10_000 * dollar(AUSD));

			assert_eq!(
				CdpEngine::calculate_collateral_ratio(
					RENBTC,
					100 * dollar(RENBTC),
					50 * dollar(AUSD),
					Price::saturating_from_rational(1 * dollar(AUSD), dollar(RENBTC)),
				),
				Ratio::saturating_from_rational(100 * 10, 50)
			);

			assert_ok!(CdpEngine::check_debit_cap(RENBTC, 99_999 * dollar(AUSD)));
			assert_eq!(
				CdpEngine::check_debit_cap(RENBTC, 100_001 * dollar(AUSD)).is_ok(),
				false
			);

			assert_ok!(CdpEngine::adjust_position(
				&AccountId::from(ALICE),
				RENBTC,
				(100 * dollar(RENBTC)) as i128,
				0
			));
			assert_eq!(
				Currencies::free_balance(RENBTC, &AccountId::from(ALICE)),
				900 * dollar(RENBTC)
			);
			assert_eq!(Loans::positions(RENBTC, AccountId::from(ALICE)).debit, 0);
			assert_eq!(
				Loans::positions(RENBTC, AccountId::from(ALICE)).collateral,
				100 * dollar(RENBTC)
			);

			assert_noop!(
				CdpEngine::settle_cdp_has_debit(AccountId::from(ALICE), RENBTC),
				module_cdp_engine::Error::<Runtime>::NoDebitValue,
			);

			assert_ok!(set_oracle_price(vec![
				(AUSD, Price::saturating_from_rational(1, 1)),
				(RENBTC, Price::saturating_from_rational(3, 1))
			]));

			assert_ok!(CdpEngine::adjust_position(
				&AccountId::from(ALICE),
				RENBTC,
				0,
				(100 * dollar(AUSD)) as i128
			));
			assert_eq!(
				Loans::positions(RENBTC, AccountId::from(ALICE)).debit,
				100 * dollar(AUSD)
			);
			assert_eq!(CdpTreasury::debit_pool(), 0);
			assert_eq!(CdpTreasury::total_collaterals(RENBTC), 0);
			assert_ok!(CdpEngine::settle_cdp_has_debit(AccountId::from(ALICE), RENBTC));

			let settle_cdp_in_debit_event = Event::CdpEngine(module_cdp_engine::Event::SettleCDPInDebit(
				RENBTC,
				AccountId::from(ALICE),
			));
			assert!(System::events()
				.iter()
				.any(|record| record.event == settle_cdp_in_debit_event));

			assert_eq!(Loans::positions(RENBTC, AccountId::from(ALICE)).debit, 0);
			assert_eq!(CdpTreasury::debit_pool(), 10 * dollar(AUSD));
			assert_eq!(CdpTreasury::total_collaterals(RENBTC), 333_333_333);
		});
}

#[test]
fn test_authority_module() {
	const AUTHORITY_ORIGIN_ID: u8 = 70u8;

	ExtBuilder::default()
		.balances(vec![
			(AccountId::from(ALICE), AUSD, 1_000 * dollar(AUSD)),
			(AccountId::from(ALICE), RENBTC, 1_000 * dollar(RENBTC)),
			(TreasuryReservePalletId::get().into_account(), AUSD, 1000 * dollar(AUSD)),
		])
		.build()
		.execute_with(|| {
			let ensure_root_call = Call::System(frame_system::Call::fill_block(Perbill::one()));
			let call = Call::Authority(orml_authority::Call::dispatch_as(
				AuthoritysOriginId::Root,
				Box::new(ensure_root_call.clone()),
			));

			// dispatch_as
			assert_ok!(Authority::dispatch_as(
				Origin::root(),
				AuthoritysOriginId::Root,
				Box::new(ensure_root_call.clone())
			));

			assert_noop!(
				Authority::dispatch_as(
					Origin::signed(AccountId::from(BOB)),
					AuthoritysOriginId::Root,
					Box::new(ensure_root_call.clone())
				),
				BadOrigin
			);

			assert_noop!(
				Authority::dispatch_as(
					Origin::signed(AccountId::from(BOB)),
					AuthoritysOriginId::Treasury,
					Box::new(ensure_root_call.clone())
				),
				BadOrigin
			);

			// schedule_dispatch
			run_to_block(1);
			// TreasuryReserve transfer
			let transfer_call = Call::Currencies(module_currencies::Call::transfer(
				AccountId::from(BOB).into(),
				AUSD,
				500 * dollar(AUSD),
			));
			let treasury_reserve_call = Call::Authority(orml_authority::Call::dispatch_as(
				AuthoritysOriginId::TreasuryReserve,
				Box::new(transfer_call.clone()),
			));
			assert_ok!(Authority::schedule_dispatch(
				Origin::root(),
				DispatchTime::At(2),
				0,
				true,
				Box::new(treasury_reserve_call.clone())
			));

			assert_ok!(Authority::schedule_dispatch(
				Origin::root(),
				DispatchTime::At(2),
				0,
				true,
				Box::new(call.clone())
			));
			System::assert_last_event(Event::Authority(orml_authority::Event::Scheduled(
				OriginCaller::Authority(DelayedOrigin {
					delay: 1,
					origin: Box::new(OriginCaller::system(RawOrigin::Root)),
				}),
				1,
			)));

			run_to_block(2);
			assert_eq!(
				Currencies::free_balance(AUSD, &TreasuryReservePalletId::get().into_account()),
				500 * dollar(AUSD)
			);
			assert_eq!(
				Currencies::free_balance(AUSD, &AccountId::from(BOB)),
				500 * dollar(AUSD)
			);

			// delay < SevenDays
			System::assert_last_event(Event::Scheduler(pallet_scheduler::Event::<Runtime>::Dispatched(
				(2, 1),
				Some([AUTHORITY_ORIGIN_ID, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0].to_vec()),
				Err(DispatchError::BadOrigin),
			)));

			// delay = SevenDays
			assert_ok!(Authority::schedule_dispatch(
				Origin::root(),
				DispatchTime::At(SevenDays::get() + 2),
				0,
				true,
				Box::new(call.clone())
			));

			run_to_block(SevenDays::get() + 2);
			System::assert_last_event(Event::Scheduler(pallet_scheduler::Event::<Runtime>::Dispatched(
				(100802, 0),
				Some([AUTHORITY_ORIGIN_ID, 192, 137, 1, 0, 0, 0, 2, 0, 0, 0].to_vec()),
				Ok(()),
			)));

			// with_delayed_origin = false
			assert_ok!(Authority::schedule_dispatch(
				Origin::root(),
				DispatchTime::At(SevenDays::get() + 3),
				0,
				false,
				Box::new(call.clone())
			));
			System::assert_last_event(Event::Authority(orml_authority::Event::Scheduled(
				OriginCaller::system(RawOrigin::Root),
				3,
			)));

			run_to_block(SevenDays::get() + 3);
			System::assert_last_event(Event::Scheduler(pallet_scheduler::Event::<Runtime>::Dispatched(
				(100803, 0),
				Some([0, 0, 3, 0, 0, 0].to_vec()),
				Ok(()),
			)));

			assert_ok!(Authority::schedule_dispatch(
				Origin::root(),
				DispatchTime::At(SevenDays::get() + 4),
				0,
				false,
				Box::new(call.clone())
			));

			// fast_track_scheduled_dispatch
			assert_ok!(Authority::fast_track_scheduled_dispatch(
				Origin::root(),
				frame_system::RawOrigin::Root.into(),
				4,
				DispatchTime::At(SevenDays::get() + 5),
			));

			// delay_scheduled_dispatch
			assert_ok!(Authority::delay_scheduled_dispatch(
				Origin::root(),
				frame_system::RawOrigin::Root.into(),
				4,
				4,
			));

			// cancel_scheduled_dispatch
			assert_ok!(Authority::schedule_dispatch(
				Origin::root(),
				DispatchTime::At(SevenDays::get() + 4),
				0,
				true,
				Box::new(call.clone())
			));
			System::assert_last_event(Event::Authority(orml_authority::Event::Scheduled(
				OriginCaller::Authority(DelayedOrigin {
					delay: 1,
					origin: Box::new(OriginCaller::system(RawOrigin::Root)),
				}),
				5,
			)));

			let schedule_origin = {
				let origin: <Runtime as orml_authority::Config>::Origin = From::from(Origin::root());
				let origin: <Runtime as orml_authority::Config>::Origin = From::from(DelayedOrigin::<
					BlockNumber,
					<Runtime as orml_authority::Config>::PalletsOrigin,
				> {
					delay: 1,
					origin: Box::new(origin.caller().clone()),
				});
				origin
			};

			let pallets_origin = schedule_origin.caller().clone();
			assert_ok!(Authority::cancel_scheduled_dispatch(Origin::root(), pallets_origin, 5));
			System::assert_last_event(Event::Authority(orml_authority::Event::Cancelled(
				OriginCaller::Authority(DelayedOrigin {
					delay: 1,
					origin: Box::new(OriginCaller::system(RawOrigin::Root)),
				}),
				5,
			)));

			assert_ok!(Authority::schedule_dispatch(
				Origin::root(),
				DispatchTime::At(SevenDays::get() + 5),
				0,
				false,
				Box::new(call.clone())
			));
			System::assert_last_event(Event::Authority(orml_authority::Event::Scheduled(
				OriginCaller::system(RawOrigin::Root),
				6,
			)));

			assert_ok!(Authority::cancel_scheduled_dispatch(
				Origin::root(),
				frame_system::RawOrigin::Root.into(),
				6
			));
			System::assert_last_event(Event::Authority(orml_authority::Event::Cancelled(
				OriginCaller::system(RawOrigin::Root),
				6,
			)));
		});
}

#[test]
fn test_nft_module() {
	ExtBuilder::default()
		.balances(vec![(AccountId::from(ALICE), ACA, 1_000 * dollar(ACA))])
		.build()
		.execute_with(|| {
			let metadata = vec![1];
			assert_eq!(Balances::free_balance(AccountId::from(ALICE)), 1_000 * dollar(ACA));
			assert_eq!(Balances::reserved_balance(AccountId::from(ALICE)), 0);
			assert_ok!(NFT::create_class(
				Origin::signed(AccountId::from(ALICE)),
				metadata.clone(),
				module_nft::Properties(
					module_nft::ClassProperty::Transferable
						| module_nft::ClassProperty::Burnable
						| module_nft::ClassProperty::Mintable
				),
				Default::default(),
			));
			let deposit =
				Proxy::deposit(1u32) + CreateClassDeposit::get() + DataDepositPerByte::get() * (metadata.len() as u128);
			assert_eq!(Balances::free_balance(&NftPalletId::get().into_sub_account(0)), 0);
			assert_eq!(
				Balances::reserved_balance(&NftPalletId::get().into_sub_account(0)),
				deposit
			);
			assert_eq!(
				Balances::free_balance(AccountId::from(ALICE)),
				1_000 * dollar(ACA) - deposit
			);
			assert_eq!(Balances::reserved_balance(AccountId::from(ALICE)), 0);
			assert_ok!(Balances::deposit_into_existing(
				&NftPalletId::get().into_sub_account(0),
				1 * (CreateTokenDeposit::get() + DataDepositPerByte::get())
			));
			assert_ok!(NFT::mint(
				Origin::signed(NftPalletId::get().into_sub_account(0)),
				MultiAddress::Id(AccountId::from(BOB)),
				0,
				metadata.clone(),
				Default::default(),
				1
			));
			assert_ok!(NFT::burn(Origin::signed(AccountId::from(BOB)), (0, 0)));
			assert_eq!(
				Balances::free_balance(AccountId::from(BOB)),
				CreateTokenDeposit::get() + DataDepositPerByte::get()
			);
			assert_noop!(
				NFT::destroy_class(
					Origin::signed(NftPalletId::get().into_sub_account(0)),
					0,
					MultiAddress::Id(AccountId::from(BOB))
				),
				pallet_proxy::Error::<Runtime>::NotFound
			);
			assert_ok!(NFT::destroy_class(
				Origin::signed(NftPalletId::get().into_sub_account(0)),
				0,
				MultiAddress::Id(AccountId::from(ALICE))
			));
			assert_eq!(
				Balances::free_balance(AccountId::from(BOB)),
				CreateTokenDeposit::get() + DataDepositPerByte::get()
			);
			assert_eq!(Balances::reserved_balance(AccountId::from(BOB)), 0);
			assert_eq!(Balances::free_balance(AccountId::from(ALICE)), 1_000 * dollar(ACA));
			assert_eq!(Balances::reserved_balance(AccountId::from(ALICE)), 0);
		});
}

#[test]
fn test_evm_accounts_module() {
	ExtBuilder::default()
		.balances(vec![(bob(), ACA, 1_000 * dollar(ACA))])
		.build()
		.execute_with(|| {
			assert_eq!(Balances::free_balance(AccountId::from(ALICE)), 0);
			assert_eq!(Balances::free_balance(bob()), 1_000 * dollar(ACA));
			assert_ok!(EvmAccounts::claim_account(
				Origin::signed(AccountId::from(ALICE)),
				EvmAccounts::eth_address(&alice_key()),
				EvmAccounts::eth_sign(&alice_key(), &AccountId::from(ALICE).encode(), &[][..])
			));
			System::assert_last_event(Event::EvmAccounts(module_evm_accounts::Event::ClaimAccount(
				AccountId::from(ALICE),
				EvmAccounts::eth_address(&alice_key()),
			)));

			// claim another eth address
			assert_noop!(
				EvmAccounts::claim_account(
					Origin::signed(AccountId::from(ALICE)),
					EvmAccounts::eth_address(&alice_key()),
					EvmAccounts::eth_sign(&alice_key(), &AccountId::from(ALICE).encode(), &[][..])
				),
				module_evm_accounts::Error::<Runtime>::AccountIdHasMapped
			);
			assert_noop!(
				EvmAccounts::claim_account(
					Origin::signed(AccountId::from(BOB)),
					EvmAccounts::eth_address(&alice_key()),
					EvmAccounts::eth_sign(&alice_key(), &AccountId::from(BOB).encode(), &[][..])
				),
				module_evm_accounts::Error::<Runtime>::EthAddressHasMapped
			);

			// evm padded address will transfer_all to origin.
			assert_eq!(Balances::free_balance(bob()), 1_000 * dollar(ACA));
			assert_eq!(Balances::free_balance(&AccountId::from(BOB)), 0);
			assert_eq!(System::providers(&bob()), 1);
			assert_eq!(System::providers(&AccountId::from(BOB)), 0);
			assert_ok!(EvmAccounts::claim_account(
				Origin::signed(AccountId::from(BOB)),
				EvmAccounts::eth_address(&bob_key()),
				EvmAccounts::eth_sign(&bob_key(), &AccountId::from(BOB).encode(), &[][..])
			));
			assert_eq!(System::providers(&bob()), 0);
			assert_eq!(System::providers(&AccountId::from(BOB)), 1);
			assert_eq!(Balances::free_balance(bob()), 0);
			assert_eq!(Balances::free_balance(&AccountId::from(BOB)), 1_000 * dollar(ACA));
		});
}

#[cfg(not(feature = "with-ethereum-compatibility"))]
#[test]
fn test_evm_module() {
	ExtBuilder::default()
		.balances(vec![
			(alice(), ACA, 1_000 * dollar(ACA)),
			(bob(), ACA, 1_000 * dollar(ACA)),
		])
		.build()
		.execute_with(|| {
			assert_eq!(Balances::free_balance(alice()), 1_000 * dollar(ACA));
			assert_eq!(Balances::free_balance(bob()), 1_000 * dollar(ACA));

			let _alice_address = EvmAccounts::eth_address(&alice_key());
			let bob_address = EvmAccounts::eth_address(&bob_key());

			let contract = deploy_contract(alice()).unwrap();
			System::assert_last_event(Event::EVM(module_evm::Event::Created(contract)));

			assert_ok!(EVM::transfer_maintainer(Origin::signed(alice()), contract, bob_address));
			System::assert_last_event(Event::EVM(module_evm::Event::TransferredMaintainer(
				contract,
				bob_address,
			)));

			// test EvmAccounts Lookup
			assert_eq!(Balances::free_balance(alice()), 999_999_896_330_000);
			assert_eq!(Balances::free_balance(bob()), 1_000 * dollar(ACA));
			let to = EvmAccounts::eth_address(&alice_key());
			assert_ok!(Currencies::transfer(
				Origin::signed(bob()),
				MultiAddress::Address20(to.0),
				ACA,
				10 * dollar(ACA)
			));
			assert_eq!(Balances::free_balance(alice()), 1_009_999_896_330_000);
			assert_eq!(Balances::free_balance(bob()), 1_000 * dollar(ACA) - 10 * dollar(ACA));
		});
}

#[cfg(feature = "with-ethereum-compatibility")]
#[test]
fn test_evm_module() {
	ExtBuilder::default()
		.balances(vec![
			(alice(), ACA, 1_000 * dollar(ACA)),
			(bob(), ACA, 1_000 * dollar(ACA)),
		])
		.build()
		.execute_with(|| {
			assert_eq!(Balances::free_balance(alice()), 1_000 * dollar(ACA));
			assert_eq!(Balances::free_balance(bob()), 1_000 * dollar(ACA));

			use std::fs::{self, File};
			use std::io::Read;

			let paths = fs::read_dir("../../runtime/mandala/tests/solidity_test").unwrap();
			let file_names = paths
				.filter_map(|entry| entry.ok().and_then(|e| e.path().to_str().map(|s| String::from(s))))
				.collect::<Vec<String>>();

			for file in file_names {
				let mut f = File::open(&file).expect("File not found");
				let mut contents = String::new();
				f.read_to_string(&mut contents)
					.expect("Something went wrong reading the file.");
				let json: serde_json::Value = serde_json::from_str(&contents).unwrap();

				let bytecode_str = serde_json::to_string(&json["bytecode"]).unwrap();
				let bytecode_str = bytecode_str.replace("\"", "");

				let bytecode = hex::decode(bytecode_str).unwrap();
				assert_ok!(EVM::create(Origin::signed(alice()), bytecode, 0, u64::MAX, u32::MAX));

				match System::events().iter().last().unwrap().event {
					Event::EVM(module_evm::Event::Created(_)) => {}
					_ => {
						println!(
							"contract {:?} create failed, event: {:?}",
							file,
							System::events().iter().last().unwrap().event
						);
						assert!(false);
					}
				};
			}
		});
}

#[test]
fn test_multicurrency_precompile_module() {
	ExtBuilder::default()
		.balances(vec![
			(
				// NetworkContractSource
				MockAddressMapping::get_account_id(&H160::from_low_u64_be(0)),
				ACA,
				(1_000_000_000_000_000_000u128),
			),
			(
				// evm alice
				MockAddressMapping::get_account_id(&alice_evm_addr()),
				ACA,
				(1_000_000_000_000_000_000u128),
			),
			(AccountId::from(ALICE), AUSD, (1_000_000_000_000_000_000u128)),
			(AccountId::from(ALICE), RENBTC, (1_000_000_000_000_000_000u128)),
			(AccountId::from(BOB), AUSD, (1_000_000_000_000_000_000u128)),
			(AccountId::from(BOB), RENBTC, (1_000_000_000_000_000_000u128)),
		])
		.build()
		.execute_with(|| {
			deploy_erc20_contracts();

			// Erc20
			assert_ok!(EvmAccounts::claim_account(
				Origin::signed(AccountId::from(ALICE)),
				EvmAccounts::eth_address(&alice_key()),
				EvmAccounts::eth_sign(&alice_key(), &AccountId::from(ALICE).encode(), &[][..])
			));
			assert_ok!(Dex::list_provisioning(
				Origin::root(),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
				10,
				100,
				100,
				1000,
				0,
			));

			// CurrencyId::DexShare(Erc20, Erc20)
			<EVM as EVMTrait<AccountId>>::set_origin(MockAddressMapping::get_account_id(&alice_evm_addr()));
			assert_ok!(Dex::add_provision(
				Origin::signed(MockAddressMapping::get_account_id(&alice_evm_addr())),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
				100,
				1000,
			));
			assert_ok!(Dex::end_provisioning(
				Origin::signed(AccountId::from(ALICE)),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
			));
			assert_eq!(
				Dex::get_liquidity_pool(
					CurrencyId::Erc20(erc20_address_0()),
					CurrencyId::Erc20(erc20_address_1())
				),
				(100, 1000)
			);

			assert_eq!(
				Currencies::total_issuance(CurrencyId::DexShare(
					DexShare::Erc20(erc20_address_0()),
					DexShare::Erc20(erc20_address_1())
				)),
				200
			);

			assert_ok!(Dex::claim_dex_share(
				Origin::signed(MockAddressMapping::get_account_id(&alice_evm_addr())),
				MockAddressMapping::get_account_id(&alice_evm_addr()),
				CurrencyId::Erc20(erc20_address_0()),
				CurrencyId::Erc20(erc20_address_1()),
			));
			assert_eq!(
				Currencies::free_balance(
					CurrencyId::DexShare(DexShare::Erc20(erc20_address_0()), DexShare::Erc20(erc20_address_1())),
					&MockAddressMapping::get_account_id(&alice_evm_addr())
				),
				200
			);

			let invoke_context = module_support::InvokeContext {
				contract: lp_erc20_evm_address(),
				sender: alice_evm_addr(),
				origin: alice_evm_addr(),
			};

			assert_eq!(
				EVMBridge::name(invoke_context),
				Ok(b"LP long string name, long string name, long string name, long string name, long string name, long string name, long string name, long string name, long string name, long string name"[..32].to_vec())
			);
			assert_eq!(
				EVMBridge::symbol(invoke_context),
				Ok(b"LP_TestToken_TestToken".to_vec())
			);
			assert_eq!(
				EVMBridge::decimals(invoke_context),
				Ok(17)
			);
			assert_eq!(
				EVMBridge::total_supply(invoke_context),
				Ok(200)
			);
			assert_eq!(
				EVMBridge::balance_of(invoke_context, alice_evm_addr()),
				Ok(200)
			);
			assert_eq!(
				EVMBridge::total_supply(invoke_context),
				Ok(200)
			);
			assert_eq!(
				EVMBridge::balance_of(invoke_context, alice_evm_addr()),
				Ok(200)
			);
			assert_eq!(
				EVMBridge::transfer(invoke_context, bob_evm_addr(), 1),
				Ok(())
			);
			assert_eq!(
				EVMBridge::balance_of(invoke_context, alice_evm_addr()),
				Ok(199)
			);
			assert_eq!(
				EVMBridge::balance_of(invoke_context, bob_evm_addr()),
				Ok(1)
			);
		});
}

#[test]
fn should_not_kill_contract_on_transfer_all() {
	ExtBuilder::default()
		.balances(vec![
			(alice(), ACA, 1_000 * dollar(ACA)),
			(bob(), ACA, 1_000 * dollar(ACA)),
		])
		.build()
		.execute_with(|| {
			// pragma solidity ^0.5.0;
			//
			// contract Test {
			// 	 constructor() public payable {
			// 	 }
			// }
			let code = hex_literal::hex!("6080604052603e8060116000396000f3fe6080604052600080fdfea265627a7a72315820e816b34c9ce8a2446f3d059b4907b4572645fde734e31dabf5465c801dcb44a964736f6c63430005110032").to_vec();

			assert_ok!(EVM::create(Origin::signed(alice()), code, 2 * dollar(ACA), 1000000000, 1000000000));

			let contract = if let Event::EVM(module_evm::Event::Created(address)) = System::events().iter().last().unwrap().event {
				address
			} else {
				panic!("deploy contract failed");
			};

			assert_eq!(Balances::free_balance(EvmAddressMapping::<Runtime>::get_account_id(&contract)), 2 * dollar(ACA));

			#[cfg(not(feature = "with-ethereum-compatibility"))]
			assert_eq!(Balances::free_balance(alice()), 997_999_899_380_000);

			#[cfg(feature = "with-ethereum-compatibility")]
			assert_eq!(Balances::free_balance(alice()), 998 * dollar(ACA));

			assert_ok!(Currencies::transfer(
				Origin::signed(EvmAddressMapping::<Runtime>::get_account_id(&contract)),
				alice().into(),
				ACA,
				2 * dollar(ACA)
			));

			assert_eq!(Balances::free_balance(EvmAddressMapping::<Runtime>::get_account_id(&contract)), 0);

			#[cfg(not(feature = "with-ethereum-compatibility"))]
			assert_eq!(Balances::free_balance(alice()), 999_999_899_380_000);

			#[cfg(feature = "with-ethereum-compatibility")]
			assert_eq!(Balances::free_balance(alice()), 1000 * dollar(ACA));

			// assert the contract account is not purged
			assert!(EVM::accounts(contract).is_some());
		});
}

#[test]
fn should_not_kill_contract_on_transfer_all_tokens() {
	ExtBuilder::default()
		.balances(vec![
			(alice(), ACA, 1_000 * dollar(ACA)),
			(alice(), AUSD, 1_000 * dollar(AUSD)),
			(bob(), ACA, 1_000 * dollar(ACA)),
		])
		.build()
		.execute_with(|| {
			// pragma solidity ^0.5.0;
			//
			// contract Test {
			// 	 constructor() public payable {
			// 	 }
			//
			// 	 function kill() public {
			// 	     selfdestruct(address(0));
			// 	 }
			// }
			let code = hex_literal::hex!("608060405260848060116000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c806341c0e1b514602d575b600080fd5b60336035565b005b600073ffffffffffffffffffffffffffffffffffffffff16fffea265627a7a72315820ed64a7551098c4afc823bee1663309079d9cb8798a6bdd71be2cd3ccee52d98e64736f6c63430005110032").to_vec();
			assert_ok!(EVM::create(Origin::signed(alice()), code, 0, 1000000000, 1000000000));
			let contract = if let Event::EVM(module_evm::Event::Created(address)) = System::events().iter().last().unwrap().event {
				address
			} else {
				panic!("deploy contract failed");
			};

			let contract_account_id = EvmAddressMapping::<Runtime>::get_account_id(&contract);

			assert_ok!(Currencies::transfer(
				Origin::signed(alice()),
				contract_account_id.clone().into(),
				AUSD,
				2 * dollar(AUSD)
			));

			assert_eq!(Currencies::free_balance(AUSD, &alice()), 998 * dollar(AUSD));
			assert_eq!(Currencies::free_balance(AUSD, &contract_account_id), 2 * dollar(AUSD));
			assert_eq!(EVM::accounts(contract).unwrap().nonce, 1);
			assert_ok!(Currencies::transfer(
				Origin::signed(contract_account_id.clone()),
				alice().into(),
				AUSD,
				2 * dollar(AUSD)
			));
			assert_eq!(Currencies::free_balance(AUSD, &contract_account_id), 0);

			assert_eq!(Currencies::free_balance(AUSD, &alice()), 1000 * dollar(AUSD));

			// assert the contract account is not purged
			assert!(EVM::accounts(contract).is_some());

			assert_ok!(EVM::call(Origin::signed(alice()), contract.clone(), hex_literal::hex!("41c0e1b5").to_vec(), 0, 1000000000, 1000000000));

			assert!(EVM::accounts(contract).is_none());

			// should be gone
			assert!(!System::account_exists(&contract_account_id));
		});
}

#[test]
fn test_vesting_use_relaychain_block_number() {
	ExtBuilder::default().build().execute_with(|| {
		let treasury: AccountId = TreasuryPalletId::get().into_account();

		assert_ok!(Balances::set_balance(
			Origin::root(),
			treasury.clone().into(),
			1_000 * dollar(ACA),
			0
		));

		assert_ok!(Vesting::vested_transfer(
			Origin::signed(treasury),
			alice().into(),
			VestingSchedule {
				start: 10,
				period: 2,
				period_count: 5,
				per_period: 3 * dollar(ACA),
			}
		));

		assert_eq!(Balances::free_balance(&alice()), 15 * dollar(ACA));
		assert_eq!(Balances::usable_balance(&alice()), 0);

		set_relaychain_block_number(10);

		assert_ok!(Vesting::claim(Origin::signed(alice())));
		assert_eq!(Balances::usable_balance(&alice()), 0);

		set_relaychain_block_number(12);

		assert_ok!(Vesting::claim(Origin::signed(alice())));
		assert_eq!(Balances::usable_balance(&alice()), 3 * dollar(ACA));

		set_relaychain_block_number(15);

		assert_ok!(Vesting::claim(Origin::signed(alice())));
		assert_eq!(Balances::usable_balance(&alice()), 6 * dollar(ACA));

		set_relaychain_block_number(20);

		assert_ok!(Vesting::claim(Origin::signed(alice())));
		assert_eq!(Balances::usable_balance(&alice()), 15 * dollar(ACA));

		set_relaychain_block_number(22);

		assert_ok!(Vesting::claim(Origin::signed(alice())));
		assert_eq!(Balances::usable_balance(&alice()), 15 * dollar(ACA));
	});
}

#[test]
fn test_session_manager_module() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Session::session_index(), 0);
		assert_eq!(SessionManager::session_duration(), 10);
		run_to_block(10);
		assert_eq!(Session::session_index(), 1);
		assert_eq!(SessionManager::session_duration(), 10);

		assert_ok!(SessionManager::schedule_session_duration(RawOrigin::Root.into(), 2, 11));

		run_to_block(19);
		assert_eq!(Session::session_index(), 1);
		assert_eq!(SessionManager::session_duration(), 10);

		run_to_block(20);
		assert_eq!(Session::session_index(), 2);
		assert_eq!(SessionManager::session_duration(), 11);

		run_to_block(31);
		assert_eq!(Session::session_index(), 3);
		assert_eq!(SessionManager::session_duration(), 11);

		assert_ok!(SessionManager::schedule_session_duration(RawOrigin::Root.into(), 4, 9));

		run_to_block(42);
		assert_eq!(Session::session_index(), 4);
		assert_eq!(SessionManager::session_duration(), 9);

		run_to_block(50);
		assert_eq!(Session::session_index(), 4);
		assert_eq!(SessionManager::session_duration(), 9);

		run_to_block(51);
		assert_eq!(Session::session_index(), 5);
		assert_eq!(SessionManager::session_duration(), 9);
	});
}

#[test]
fn treasury_should_take_xcm_execution_revenue() {
	ExtBuilder::default().build().execute_with(|| {
		let dot_amount = 1000 * dollar(DOT);
		let actual_amount = 9999999760000;
		let shallow_weight = 3000000;
		let origin = MultiLocation::X1(Junction::Parent);

		// receive relay chain token
		let mut msg = Xcm::<Call>::ReserveAssetDeposit {
			assets: vec![MultiAsset::ConcreteFungible {
				id: MultiLocation::X1(Junction::Parent),
				amount: dot_amount,
			}],
			effects: vec![
				BuyExecution {
					fees: MultiAsset::All,
					weight: 0,
					debt: shallow_weight,
					halt_on_error: true,
					xcm: vec![],
				},
				DepositAsset {
					assets: vec![MultiAsset::All],
					dest: MultiLocation::X1(Junction::AccountId32 {
						network: NetworkId::Any,
						id: ALICE,
					}),
				},
			],
		};
		use xcm_executor::traits::WeightBounds;
		let debt = <XcmConfig as xcm_executor::Config>::Weigher::shallow(&mut msg).unwrap_or_default();
		let deep = <XcmConfig as xcm_executor::Config>::Weigher::deep(&mut msg).unwrap_or_default();
		// println!("debt = {:?}, deep = {:?}", debt, deep);
		assert_eq!(debt, shallow_weight);

		assert_eq!(Tokens::free_balance(DOT, &ALICE.into()), 0);
		assert_ok!(Currencies::deposit(
			DOT,
			&TreasuryAccount::get(),
			ExistentialDeposits::get(&DOT)
		));
		assert_eq!(
			Tokens::free_balance(DOT, &TreasuryAccount::get()),
			ExistentialDeposits::get(&DOT)
		);

		let weight_limit = debt + deep + 1;
		assert_eq!(
			XcmExecutor::<XcmConfig>::execute_xcm(origin, msg, weight_limit),
			Outcome::Complete(shallow_weight)
		);

		assert_eq!(Tokens::free_balance(DOT, &ALICE.into()), actual_amount);
		assert_eq!(
			Tokens::free_balance(DOT, &TreasuryAccount::get()),
			ExistentialDeposits::get(&DOT) + dot_amount - actual_amount
		);
	});
}

// #[test]
// fn receive_cross_chain_assets() {
// 	ExtBuilder::default().build().execute_with(|| {
// 		let dot_amount = 1000 * dollar(DOT);

// 		// receive relay chain token
// 		let msg: VersionedXcm = Xcm::ReserveAssetDeposit {
// 			assets: vec![MultiAsset::ConcreteFungible {
// 				id: MultiLocation::X1(Junction::Parent),
// 				amount: dot_amount,
// 			}],
// 			effects: vec![Order::DepositAsset {
// 				assets: vec![MultiAsset::All],
// 				dest: MultiLocation::X1(Junction::AccountId32 {
// 					network: NetworkId::Named("acala".into()),
// 					id: ALICE,
// 				}),
// 			}],
// 		}
// 		.into();
// 		XcmHandler::handle_downward_message(InboundDownwardMessage {
// 			sent_at: 10,
// 			msg: msg.encode(),
// 		});
// 		assert_eq!(Tokens::free_balance(DOT, &ALICE.into()), dot_amount);

// 		let sibling_para_id = 5000;
// 		let sibling_parachain_acc: AccountId = Sibling::from(sibling_para_id).into_account();

// 		// receive owned token
// 		let aca_amount = 1000 * dollar(ACA);
// 		assert_ok!(Currencies::deposit(ACA, &sibling_parachain_acc, 1100 * dollar(ACA)));

// 		let para_id: u32 = ParachainInfo::get().into();
// 		let msg1: VersionedXcm = Xcm::WithdrawAsset {
// 			assets: vec![MultiAsset::ConcreteFungible {
// 				id: MultiLocation::X3(
// 					Junction::Parent,
// 					Junction::Parachain { id: para_id },
// 					Junction::GeneralKey(CurrencyId::Token(TokenSymbol::ACA).encode()),
// 				),
// 				amount: aca_amount,
// 			}],
// 			effects: vec![Order::DepositAsset {
// 				assets: vec![MultiAsset::All],
// 				dest: MultiLocation::X1(Junction::AccountId32 {
// 					network: NetworkId::Named("acala".into()),
// 					id: ALICE,
// 				}),
// 			}],
// 		}
// 		.into();
// 		XcmHandler::handle_xcm_message(sibling_para_id.into(), 10, msg1);
// 		assert_eq!(Currencies::free_balance(ACA, &sibling_parachain_acc), 100 * dollar(ACA));
// 		assert_eq!(Currencies::free_balance(ACA, &ALICE.into()), aca_amount);
// 	});
// }

#[test]
fn currency_id_convert() {
	ExtBuilder::default().build().execute_with(|| {
		let id: u32 = ParachainInfo::get().into();

		assert_eq!(CurrencyIdConvert::convert(DOT), Some(X1(Parent)));
		assert_eq!(
			CurrencyIdConvert::convert(ACA),
			Some(X3(Parent, Parachain(id), GeneralKey(ACA.encode())))
		);
		assert_eq!(
			CurrencyIdConvert::convert(AUSD),
			Some(X3(Parent, Parachain(id), GeneralKey(AUSD.encode())))
		);
		assert_eq!(
			CurrencyIdConvert::convert(LDOT),
			Some(X3(Parent, Parachain(id), GeneralKey(LDOT.encode())))
		);
		assert_eq!(
			CurrencyIdConvert::convert(RENBTC),
			Some(X3(Parent, Parachain(id), GeneralKey(RENBTC.encode())))
		);
		assert_eq!(CurrencyIdConvert::convert(KAR), None);
		assert_eq!(CurrencyIdConvert::convert(KUSD), None);
		assert_eq!(CurrencyIdConvert::convert(KSM), None);
		assert_eq!(CurrencyIdConvert::convert(LKSM), None);

		assert_eq!(CurrencyIdConvert::convert(X1(Parent)), Some(DOT));
		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id), GeneralKey(ACA.encode()))),
			Some(ACA)
		);
		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id), GeneralKey(AUSD.encode()))),
			Some(AUSD)
		);
		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id), GeneralKey(LDOT.encode()))),
			Some(LDOT)
		);
		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id), GeneralKey(RENBTC.encode()))),
			Some(RENBTC)
		);
		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id), GeneralKey(KAR.encode()))),
			None
		);
		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id), GeneralKey(KUSD.encode()))),
			None
		);
		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id), GeneralKey(KSM.encode()))),
			None
		);
		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id), GeneralKey(LKSM.encode()))),
			None
		);

		assert_eq!(
			CurrencyIdConvert::convert(X3(Parent, Parachain(id + 1), GeneralKey(ACA.encode()))),
			None
		);

		assert_eq!(
			CurrencyIdConvert::convert(MultiAsset::ConcreteFungible {
				id: X3(Parent, Parachain(id), GeneralKey(ACA.encode())),
				amount: 1
			}),
			Some(ACA)
		);
	});
}

#[test]
fn parachain_subaccounts_are_unique() {
	ExtBuilder::default().build().execute_with(|| {
		let parachain: AccountId = ParachainInfo::parachain_id().into_account();
		assert_eq!(
			parachain,
			hex_literal::hex!["7061726164000000000000000000000000000000000000000000000000000000"].into()
		);

		assert_eq!(
			create_x2_parachain_multilocation(0),
			MultiLocation::X2(
				Junction::Parent,
				Junction::AccountId32 {
					network: NetworkId::Polkadot,
					id: hex_literal::hex!["00465d6ab005c2fd8c4e0bf22a60fe3ce5ff035072ec74679f4babb4c6f00833"].into(),
				}
			)
		);
		assert_eq!(
			create_x2_parachain_multilocation(1),
			MultiLocation::X2(
				Junction::Parent,
				Junction::AccountId32 {
					network: NetworkId::Polkadot,
					id: hex_literal::hex!["f0516bdbb7c54cac650736b9891b59242dd8e4e1c14df46dc39550fbb407dbe9"].into(),
				}
			)
		);
	});
}
