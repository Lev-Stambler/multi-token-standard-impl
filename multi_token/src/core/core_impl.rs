use super::resolver::MultiTokenResolver;
use crate::multi_token::core::MultiTokenCore;
use crate::multi_token::metadata::TokenMetadata;
use crate::multi_token::token::{Token, TokenId};
use crate::multi_token::utils::{hash_account_id, refund_approved_account_ids, refund_deposit};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, TreeMap, UnorderedSet};
use near_sdk::json_types::{Base64VecU8, ValidAccountId};
use near_sdk::{
	assert_one_yocto, env, ext_contract, log, AccountId, Balance, BorshStorageKey, CryptoHash,
	Gas, IntoStorageKey, PromiseOrValue, PromiseResult, StorageUsage,
};
use std::collections::HashMap;

const GAS_FOR_RESOLVE_TRANSFER: Gas = 5_000_000_000_000;
const GAS_FOR_FT_TRANSFER_CALL: Gas = 25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER;

const NO_DEPOSIT: Balance = 0;

#[ext_contract(ext_self)]
trait MultiResolver {
	fn multi_resolve_transfer(
		&mut self,
		previous_owner_id: AccountId,
		receiver_id: AccountId,
		token_id: TokenId,
		approved_account_ids: Option<HashMap<AccountId, u64>>,
	) -> bool;
}

#[ext_contract(ext_receiver)]
pub trait MultiReceiver {
	/// Returns true if token should be returned to `sender_id`
	fn multi_on_transfer(
		&mut self,
		sender_id: AccountId,
		previous_owner_id: AccountId,
		token_id: TokenId,
		msg: String,
	) -> PromiseOrValue<bool>;
}

/// Implementation of multi-token standard.
/// There are next traits that any contract may implement:
///     - MultiTokenCore -- interface with multi_transfer/balance/supply methods. MultiToken provides methods for it.
///     - MultiTokenApproval -- interface with multi_approve methods. MultiToken provides methods for it.
///     - MultiTokenMetadata -- return metadata for the token in NEP-177, up to contract to implement.
///
/// For example usage, see examples/non-fungible-token/src/lib.rs.
#[derive(BorshDeserialize, BorshSerialize)]
pub struct MultiToken {
	// owner of contract; this is the only account allowed to call `mint`
	pub owner_id: AccountId,

	// The storage size in bytes for each new token
	pub extra_storage_in_bytes_per_token: StorageUsage,

	// always required
	pub owner_by_id: TreeMap<TokenId, AccountId>,


	// required by metadata extension
	pub token_metadata_by_id: Option<LookupMap<TokenId, TokenMetadata>>,

	// required by approval extension
	pub approvals_by_id: Option<LookupMap<TokenId, HashMap<AccountId, u64>>>,
	pub next_approval_id_by_id: Option<LookupMap<TokenId, u64>>,
}

impl MultiToken {
	pub fn new<Q, R, S, T>(
		owner_by_id_prefix: Q,
		owner_id: ValidAccountId,
		token_metadata_prefix: Option<R>,
		approval_prefix: Option<T>,
	) -> Self
	where
		Q: IntoStorageKey,
		R: IntoStorageKey,
		T: IntoStorageKey,
	{
		let (approvals_by_id, next_approval_id_by_id) =
			if let Some(prefix) = approval_prefix {
				let prefix: Vec<u8> = prefix.into_storage_key();
				(
					Some(LookupMap::new(prefix.clone())),
					Some(LookupMap::new([prefix, "n".into()].concat())),
				)
			} else {
				(None, None)
			};

		let mut this = Self {
			owner_id: owner_id.into(),
			extra_storage_in_bytes_per_token: 0,
			owner_by_id: TreeMap::new(owner_by_id_prefix),
			token_metadata_by_id: token_metadata_prefix.map(LookupMap::new),
			approvals_by_id,
			next_approval_id_by_id,
		};
		this.measure_min_token_storage_cost();
		this
	}

	fn measure_min_token_storage_cost(&mut self) {
		let initial_storage_usage = env::storage_usage();
		let tmp_token_id = "a".repeat(64); // TODO: what's a reasonable max TokenId length?
		let tmp_owner_id = "a".repeat(64);
		// 1. set some dummy data
		self.owner_by_id.insert(&tmp_token_id, &tmp_owner_id);
		if let Some(token_metadata_by_id) = &mut self.token_metadata_by_id {
			token_metadata_by_id.insert(
				&tmp_token_id,
				&TokenMetadata {
					title: Some("a".repeat(64)),
					description: Some("a".repeat(64)),
					media: Some("a".repeat(64)),
					media_hash: Some(Base64VecU8::from(
						"a".repeat(64).as_bytes().to_vec(),
					)),
					copies: Some(1),
					issued_at: None,
					expires_at: None,
					starts_at: None,
					updated_at: None,
					extra: None,
					reference: None,
					reference_hash: None,
				},
			);
		}

		if let Some(approvals_by_id) = &mut self.approvals_by_id {
			let mut approvals = HashMap::new();
			approvals.insert(tmp_owner_id.clone(), 1u64);
			approvals_by_id.insert(&tmp_token_id, &approvals);
		}
		if let Some(next_approval_id_by_id) = &mut self.next_approval_id_by_id {
			next_approval_id_by_id.insert(&tmp_token_id, &1u64);
		}

		// 2. see how much space it took
		self.extra_storage_in_bytes_per_token =
			env::storage_usage() - initial_storage_usage;
		// 3. roll it all back
		if let Some(next_approval_id_by_id) = &mut self.next_approval_id_by_id {
			next_approval_id_by_id.remove(&tmp_token_id);
		}
		if let Some(approvals_by_id) = &mut self.approvals_by_id {
			approvals_by_id.remove(&tmp_token_id);
		}
		if let Some(token_metadata_by_id) = &mut self.token_metadata_by_id {
			token_metadata_by_id.remove(&tmp_token_id);
		}
		self.owner_by_id.remove(&tmp_token_id);
	}


	    /// Transfer token_id from `from` to `to`
    ///
    /// Do not perform any safety checks or do any logging
    pub fn internal_transfer_unguarded(
        &mut self,
        token_id: &TokenId,
	amount: u128,
        from: &AccountId,
        to: &AccountId,
    ) {
        // update owner
        self.owner_by_id.insert(token_id, to);
    }

    /// Transfer from current owner to receiver_id, checking that sender is allowed to transfer.
    /// Clear approvals, if approval extension being used.
    /// Return previous owner and approvals.
    pub fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        token_id: &TokenId,
	amount: u128,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) -> (AccountId, Option<HashMap<AccountId, u64>>) {
        let owner_id = self.owner_by_id.get(token_id).expect("Token not found");

        // clear approvals, if using Approval Management extension
        // this will be rolled back by a panic if sending fails
        let approved_account_ids =
            self.approvals_by_id.as_mut().and_then(|by_id| by_id.remove(&token_id));

        // check if authorized
        if sender_id != &owner_id {
            // if approval extension is NOT being used, or if token has no approved accounts
            if approved_account_ids.is_none() {
                env::panic(b"Unauthorized")
            }

            // Approval extension is being used; get approval_id for sender.
            let actual_approval_id = approved_account_ids.as_ref().unwrap().get(sender_id);

            // Panic if sender not approved at all
            if actual_approval_id.is_none() {
                env::panic(b"Sender not approved");
            }

            // If approval_id included, check that it matches
            if let Some(enforced_approval_id) = approval_id {
                let actual_approval_id = actual_approval_id.unwrap();
                assert_eq!(
                    actual_approval_id, &enforced_approval_id,
                    "The actual approval_id {} is different from the given approval_id {}",
                    actual_approval_id, enforced_approval_id,
                );
            }
        }

        assert_ne!(&owner_id, receiver_id, "Current and next owner must differ");

        self.internal_transfer_unguarded(&token_id, amount, &owner_id, &receiver_id);

        log!("Transfer {} from {} to {}", token_id, sender_id, receiver_id);
        if let Some(memo) = memo {
            log!("Memo: {}", memo);
        }

        // return previous owner & approvals
        (owner_id, approved_account_ids)
    }

}

impl MultiTokenCore for MultiToken {

	fn multi_transfer(&mut self,
		receiver_id: ValidAccountId,
		token_id: TokenId, 
		amount: u128, 
		approval_id: Option<u64>, 
		memo: Option<String>) {

		assert_one_yocto();
		let sender_id = env::predecessor_account_id();
		self.internal_transfer(&sender_id, receiver_id.as_ref(), &token_id, amount, approval_id, memo);
	};

	fn multi_transfer_call(&mut self,
		receiver_id: ValidAccountId,
		token_id: TokenId,
		amount: u128,
		approval_id: Option<u64>,
		memo: Option<String>,
		msg: String,
	) ->PromiseOrValue<bool> {
		assert_one_yocto();
		let sender_id = env::predecessor_account_id();
		let (old_owner, old_approvals) =
		    self.internal_transfer(&sender_id, receiver_id.as_ref(), &token_id, amount, approval_id, memo);
		// Initiating receiver's call and the callback
		ext_receiver::multi_on_transfer(
		    sender_id.clone(),
		    old_owner.clone(),
		    token_id.clone(),
		    msg,
		    receiver_id.as_ref(),
		    NO_DEPOSIT,
		    env::prepaid_gas() - GAS_FOR_FT_TRANSFER_CALL,
		)
		.then(ext_self::multi_resolve_transfer(
		    old_owner,
		    receiver_id.into(),
		    token_id,
		    amount,
		    old_approvals,
		    &env::current_account_id(),
		    NO_DEPOSIT,
		    GAS_FOR_RESOLVE_TRANSFER,
		))
		.into()
	}

	fn multi_token(self, token_id: TokenId) -> Option<Token> {
		let owner_id = self.owner_by_id.get(&token_id)?;
		let metadata = self.token_metadata_by_id.and_then(|by_id| by_id.get(&token_id));
		let approved_account_ids = self
		    .approvals_by_id
		    .and_then(|by_id| by_id.get(&token_id).or_else(|| Some(HashMap::new())));
		Some(Token { token_id, owner_id, metadata, approved_account_ids })
	    }



}