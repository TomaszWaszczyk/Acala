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

#![cfg(feature = "runtime-benchmarks")]

use sp_runtime::traits::AccountIdConversion;

pub mod utils;

// module benchmarking
pub mod auction_manager;
pub mod cdp_engine;
pub mod cdp_treasury;
pub mod collator_selection;
pub mod currencies;
pub mod dex;
pub mod emergency_shutdown;
pub mod evm;
pub mod evm_accounts;
pub mod homa;
pub mod honzon;
pub mod incentives;
pub mod nominees_election;
pub mod prices;
pub mod session_manager;
pub mod transaction_payment;

// orml benchmarking
pub mod auction;
pub mod authority;
pub mod gradually_update;
pub mod oracle;
pub mod tokens;
pub mod vesting;

// ecosystem benchmarking
pub mod chainsafe_transfer;

pub fn get_treasury_account() -> super::AccountId {
	super::TreasuryPalletId::get().into_account()
}
