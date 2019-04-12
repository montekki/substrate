// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <http://www.gnu.org/licenses/>.

// TODO: #1417 Add more integration tests
// also remove the #![allow(unused)] below.

#![allow(unused)]

use runtime_io::with_externalities;
use runtime_primitives::testing::{Digest, DigestItem, H256, Header, UintAuthorityId};
use runtime_primitives::traits::{BlakeTwo256, IdentityLookup};
use runtime_primitives::BuildStorage;
use runtime_io;
use srml_support::{storage::child, StorageMap, assert_ok, impl_outer_event, impl_outer_dispatch,
	impl_outer_origin, traits::Currency};
use substrate_primitives::Blake2Hasher;
use system::{self, Phase, EventRecord};
use {wabt, balances, consensus};
use hex_literal::*;
use assert_matches::assert_matches;
use crate::{
	ContractAddressFor, GenesisConfig, Module, RawEvent,
	Trait, ComputeDispatchFee, TrieIdGenerator, TrieId,
	ContractInfoOf, ContractInfo, RawAliveContractInfo,
};
use substrate_primitives::storage::well_known_keys;
use parity_codec::{Encode, Decode, KeyedVec};
use std::sync::atomic::{AtomicUsize, Ordering};

mod contract {
	// Re-export contents of the root. This basically
	// needs to give a name for the current crate.
	// This hack is required for `impl_outer_event!`.
	pub use super::super::*;
	use srml_support::impl_outer_event;
}
impl_outer_event! {
	pub enum MetaEvent for Test {
		balances<T>, contract<T>,
	}
}
impl_outer_origin! {
	pub enum Origin for Test { }
}
impl_outer_dispatch! {
	pub enum Call for Test where origin: Origin {
		balances::Balances,
		contract::Contract,
	}
}

#[derive(Clone, Eq, PartialEq)]
pub struct Test;
impl system::Trait for Test {
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type Digest = Digest;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = MetaEvent;
	type Log = DigestItem;
}
impl balances::Trait for Test {
	type Balance = u64;
	type OnFreeBalanceZero = Contract;
	type OnNewAccount = ();
	type Event = MetaEvent;
	type TransactionPayment = ();
	type DustRemoval = ();
	type TransferPayment = ();
}
impl timestamp::Trait for Test {
	type Moment = u64;
	type OnTimestampSet = ();
}
impl consensus::Trait for Test {
	type Log = DigestItem;
	type SessionKey = UintAuthorityId;
	type InherentOfflineReport = ();
}
impl Trait for Test {
	type Currency = Balances;
	type Call = Call;
	type Gas = u64;
	type DetermineContractAddress = DummyContractAddressFor;
	type Event = MetaEvent;
	type ComputeDispatchFee = DummyComputeDispatchFee;
	type TrieIdGenerator = DummyTrieIdGenerator;
	type GasPayment = ();
}

type Balances = balances::Module<Test>;
type Contract = Module<Test>;
type System = system::Module<Test>;

pub struct DummyContractAddressFor;
impl ContractAddressFor<H256, u64> for DummyContractAddressFor {
	fn contract_address_for(_code_hash: &H256, _data: &[u8], origin: &u64) -> u64 {
		*origin + 1
	}
}

static KEY_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct DummyTrieIdGenerator;
impl TrieIdGenerator<u64> for DummyTrieIdGenerator {
	fn trie_id(account_id: &u64) -> TrieId {
		let mut res = KEY_COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes().to_vec();
		res.extend_from_slice(&account_id.to_le_bytes());
		res
	}
}

pub struct DummyComputeDispatchFee;
impl ComputeDispatchFee<Call, u64> for DummyComputeDispatchFee {
	fn compute_dispatch_fee(call: &Call) -> u64 {
		69
	}
}

const ALICE: u64 = 1;
const BOB: u64 = 2;
const CHARLIE: u64 = 3;

pub struct ExtBuilder {
	existential_deposit: u64,
	gas_price: u64,
	block_gas_limit: u64,
	transfer_fee: u64,
	creation_fee: u64,
}
impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			existential_deposit: 0,
			gas_price: 2,
			block_gas_limit: 100_000_000,
			transfer_fee: 0,
			creation_fee: 0,
		}
	}
}
impl ExtBuilder {
	pub fn existential_deposit(mut self, existential_deposit: u64) -> Self {
		self.existential_deposit = existential_deposit;
		self
	}
	pub fn gas_price(mut self, gas_price: u64) -> Self {
		self.gas_price = gas_price;
		self
	}
	pub fn block_gas_limit(mut self, block_gas_limit: u64) -> Self {
		self.block_gas_limit = block_gas_limit;
		self
	}
	pub fn transfer_fee(mut self, transfer_fee: u64) -> Self {
		self.transfer_fee = transfer_fee;
		self
	}
	pub fn creation_fee(mut self, creation_fee: u64) -> Self {
		self.creation_fee = creation_fee;
		self
	}
	pub fn build(self) -> runtime_io::TestExternalities<Blake2Hasher> {
		let mut t = system::GenesisConfig::<Test>::default()
			.build_storage()
			.unwrap()
			.0;
		t.extend(
			balances::GenesisConfig::<Test> {
				transaction_base_fee: 0,
				transaction_byte_fee: 0,
				balances: vec![],
				existential_deposit: self.existential_deposit,
				transfer_fee: self.transfer_fee,
				creation_fee: self.creation_fee,
				vesting: vec![],
			}
			.build_storage()
			.unwrap()
			.0,
		);
		t.extend(
			GenesisConfig::<Test> {
				signed_claim_handicap: 2,
				rent_byte_price: 4,
				rent_deposit_offset: 1000,
				storage_size_offset: 8,
				surcharge_reward: 150,
				tombstone_deposit: 16,
				transaction_base_fee: 0,
				transaction_byte_fee: 0,
				transfer_fee: self.transfer_fee,
				creation_fee: self.creation_fee,
				contract_fee: 21,
				call_base_fee: 135,
				create_base_fee: 175,
				gas_price: self.gas_price,
				max_depth: 100,
				block_gas_limit: self.block_gas_limit,
				current_schedule: Default::default(),
			}
			.build_storage()
			.unwrap()
			.0,
		);
		runtime_io::TestExternalities::new(t)
	}
}

#[test]
fn refunds_unused_gas() {
	with_externalities(&mut ExtBuilder::default().build(), || {
		Balances::deposit_creating(&0, 100_000_000);

		assert_ok!(Contract::call(
			Origin::signed(0),
			1,
			0,
			100_000,
			Vec::new()
		));

		assert_eq!(Balances::free_balance(&0), 100_000_000 - (2 * 135));
	});
}

#[test]
fn account_removal_removes_storage() {
	let unique_id1 = b"unique_id1";
	let unique_id2 = b"unique_id2";
	with_externalities(
		&mut ExtBuilder::default().existential_deposit(100).build(),
		|| {
			// Set up two accounts with free balance above the existential threshold.
			{
				Balances::deposit_creating(&1, 110);
				ContractInfoOf::<Test>::insert(1, &ContractInfo::Alive(RawAliveContractInfo {
					trie_id: unique_id1.to_vec(),
					storage_size: Contract::storage_size_offset(),
					deduct_block: System::block_number(),
					code_hash: H256::repeat_byte(1),
					rent_allowance: 40,
				}));
				child::put(&unique_id1[..], &b"foo".to_vec(), &b"1".to_vec());
				assert_eq!(child::get(&unique_id1[..], &b"foo".to_vec()), Some(b"1".to_vec()));
				child::put(&unique_id1[..], &b"bar".to_vec(), &b"2".to_vec());

				Balances::deposit_creating(&2, 110);
				ContractInfoOf::<Test>::insert(2, &ContractInfo::Alive(RawAliveContractInfo {
					trie_id: unique_id2.to_vec(),
					storage_size: Contract::storage_size_offset(),
					deduct_block: System::block_number(),
					code_hash: H256::repeat_byte(2),
					rent_allowance: 40,
				}));
				child::put(&unique_id2[..], &b"hello".to_vec(), &b"3".to_vec());
				child::put(&unique_id2[..], &b"world".to_vec(), &b"4".to_vec());
			}

			// Transfer funds from account 1 of such amount that after this transfer
			// the balance of account 1 will be below the existential threshold.
			//
			// This should lead to the removal of all storage associated with this account.
			assert_ok!(Balances::transfer(Origin::signed(1), 2, 20));

			// Verify that all entries from account 1 is removed, while
			// entries from account 2 is in place.
			{
				assert_eq!(child::get_raw(&unique_id1[..], &b"foo".to_vec()), None);
				assert_eq!(child::get_raw(&unique_id1[..], &b"bar".to_vec()), None);

				assert_eq!(
					child::get(&unique_id2[..], &b"hello".to_vec()),
					Some(b"3".to_vec())
				);
				assert_eq!(
					child::get(&unique_id2[..], &b"world".to_vec()),
					Some(b"4".to_vec())
				);
			}
		},
	);
}

const CODE_RETURN_FROM_START_FN: &str = r#"
(module
	(import "env" "ext_return" (func $ext_return (param i32 i32)))
	(import "env" "ext_deposit_event" (func $ext_deposit_event (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(start $start)
	(func $start
		(call $ext_deposit_event
			(i32.const 8)
			(i32.const 4)
		)
		(call $ext_return
			(i32.const 8)
			(i32.const 4)
		)
		(unreachable)
	)

	(func (export "call")
		(unreachable)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\01\02\03\04")
)
"#;
const HASH_RETURN_FROM_START_FN: [u8; 32] = hex!("abb4194bdea47b2904fe90b4fd674bd40d96f423956627df8c39d2b1a791ab9d");

#[test]
fn instantiate_and_call_and_deposit_event() {
	let wasm = wabt::wat2wasm(CODE_RETURN_FROM_START_FN).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(100).build(),
		|| {
			Balances::deposit_creating(&ALICE, 1_000_000);

			assert_ok!(Contract::put_code(
				Origin::signed(ALICE),
				100_000,
				wasm,
			));

			// Check at the end to get hash on error easily
			let creation = Contract::create(
				Origin::signed(ALICE),
				100,
				100_000,
				HASH_RETURN_FROM_START_FN.into(),
				vec![],
			);

			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_RETURN_FROM_START_FN.into())),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(BOB, 100)
					)
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Transfer(ALICE, BOB, 100))
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Contract(BOB, vec![1, 2, 3, 4]))
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Instantiated(ALICE, BOB))
				}
			]);

			assert_ok!(creation);
			assert!(ContractInfoOf::<Test>::exists(BOB));
		},
	);
}

const CODE_DISPATCH_CALL: &str = r#"
(module
	(import "env" "ext_dispatch_call" (func $ext_dispatch_call (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func (export "call")
		(call $ext_dispatch_call
			(i32.const 8) ;; Pointer to the start of encoded call buffer
			(i32.const 11) ;; Length of the buffer
		)
	)
	(func (export "deploy"))

	(data (i32.const 8) "\00\00\03\00\00\00\00\00\00\00\C8")
)
"#;
const HASH_DISPATCH_CALL: [u8; 32] = hex!("49dfdcaf9c1553be10634467e95b8e71a3bc15a4f8bf5563c0312b0902e0afb9");

#[test]
fn dispatch_call() {
	// This test can fail due to the encoding changes. In case it becomes too annoying
	// let's rewrite so as we use this module controlled call or we serialize it in runtime.
	let encoded = parity_codec::Encode::encode(&Call::Balances(balances::Call::transfer(CHARLIE, 50)));
	assert_eq!(&encoded[..], &hex!("00000300000000000000C8")[..]);

	let wasm = wabt::wat2wasm(CODE_DISPATCH_CALL).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			Balances::deposit_creating(&ALICE, 1_000_000);

			assert_ok!(Contract::put_code(
				Origin::signed(ALICE),
				100_000,
				wasm,
			));

			// Let's keep this assert even though it's redundant. If you ever need to update the
			// wasm source this test will fail and will show you the actual hash.
			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_DISPATCH_CALL.into())),
				},
			]);

			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				100,
				100_000,
				HASH_DISPATCH_CALL.into(),
				vec![],
			));

			assert_ok!(Contract::call(
				Origin::signed(ALICE),
				BOB, // newly created account
				0,
				100_000,
				vec![],
			));

			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_DISPATCH_CALL.into())),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(BOB, 100)
					)
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Transfer(ALICE, BOB, 100))
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Instantiated(ALICE, BOB))
				},

				// Dispatching the call.
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(CHARLIE, 50)
					)
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::Transfer(BOB, CHARLIE, 50, 0)
					)
				},

				// Event emited as a result of dispatch.
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Dispatched(BOB, true))
				}
			]);
		},
	);
}

/// Call function is compiled with https://webassembly.studio/:
/// ```C
/// #define WASM_EXPORT __attribute__((visibility("default")))
///
/// extern void input_copy(int *ptr, int offset, int len);
/// extern void set_storage(int *key, int r, int *v, int len);
///
/// extern int a;
/// WASM_EXPORT
/// int main() {
///   int do_set_storage, key, value_non_null, value_len;
///   input_copy(&do_set_storage, 0, 4);
///   input_copy(&key, 4, 4);
///   input_copy(&value_non_null, 8, 4);
///   input_copy(&value_len, 12, 4);
///
///   if (do_set_storage) {
///     set_storage(&key, value_non_null, 0, value_len);
///   }
/// }
/// ```
const CODE_SET_RENT: &str = r#"
(module
	(import "env" "ext_set_storage" (func $ext_set_storage (param i32 i32 i32 i32)))
	(import "env" "ext_set_rent_allowance" (func $ext_set_rent_allowance (param i32 i32)))
	(import "env" "ext_input_size" (func $ext_input_size (result i32)))
	(import "env" "ext_input_copy" (func $ext_input_copy (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(start $start)
	(func $start
		(local $input_size i32)
		(call $ext_set_storage
			(i32.const 0)
			(i32.const 1)
			(i32.const 0)
			(i32.const 4)
		)
		(set_local $input_size
			(call $ext_input_size)
		)
		(call $ext_input_copy
			(i32.const 0)
			(i32.const 0)
			(get_local $input_size)
		)
		(call $ext_set_rent_allowance
			(i32.const 0)
			(get_local $input_size)
		)
	)
	(func (export "call")
		(call $ext_input_copy
			(i32.const 0)
			(i32.const 0)
			(i32.const 16)
		)
		(call $ext_set_storage
			(i32.const 0)
			(i32.load (4)
			($p2)
			($p3)
		)
	)
	(func (export "deploy"))
	(data (i32.const 0) "\00\01\02\03\04\05\06\07")
)
"#;
const HASH_SET_RENT: [u8; 32] = hex!("2bf4122e304b9bcc98dd691561149b9840de11b3557900829c9d1c9b6ae505d7");

#[test]
fn rent() {
	let wasm = wabt::wat2wasm(CODE_SET_RENT).unwrap();

	with_externalities(
		&mut ExtBuilder::default().existential_deposit(50).build(),
		|| {
			Balances::deposit_creating(&ALICE, 1_000_000);

			assert_ok!(Contract::put_code(
				Origin::signed(ALICE),
				100_000,
				wasm,
			));

			// Let's keep this assert even though it's redundant. If you ever need to update the
			// wasm source this test will fail and will show you the actual hash.
			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_SET_RENT.into())),
				},
			]);

			assert_ok!(Contract::create(
				Origin::signed(ALICE),
				100,
				100_000,
				HASH_SET_RENT.into(),
				10u64.to_le_bytes().to_vec(),
			));

			let bob_contract = super::ContractInfoOf::<Test>::get(BOB).unwrap()
				.get_alive().unwrap();

			assert_eq!(bob_contract.rent_allowance, 10);
			assert_eq!(bob_contract.storage_size, Contract::storage_size_offset() + 4);

			// assert_ok!(Contract::call(
			// 	Origin::signed(ALICE),
			// 	BOB, // newly created account
			// 	0,
			// 	100_000,
			// 	vec![],
			// ));

			assert_eq!(System::events(), vec![
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(balances::RawEvent::NewAccount(1, 1_000_000)),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::CodeStored(HASH_SET_RENT.into())),
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::balances(
						balances::RawEvent::NewAccount(BOB, 100)
					)
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Transfer(ALICE, BOB, 100))
				},
				EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: MetaEvent::contract(RawEvent::Instantiated(ALICE, BOB))
				},

				// // Dispatching the call.
				// EventRecord {
				// 	phase: Phase::ApplyExtrinsic(0),
				// 	event: MetaEvent::balances(
				// 		balances::RawEvent::NewAccount(CHARLIE, 50)
				// 	)
				// },
				// EventRecord {
				// 	phase: Phase::ApplyExtrinsic(0),
				// 	event: MetaEvent::balances(
				// 		balances::RawEvent::Transfer(BOB, CHARLIE, 50, 0)
				// 	)
				// },

				// // Event emited as a result of dispatch.
				// EventRecord {
				// 	phase: Phase::ApplyExtrinsic(0),
				// 	event: MetaEvent::contract(RawEvent::Dispatched(BOB, true))
				// }
			]);
		},
	);
}
