/// The core methods for a basic semi fungible token. Extension standards may be
/// added in addition to this macro.
///

#[macro_export]
macro_rules! impl_multi_token_core {
    ($contract: ident, $token: ident) => {
        use $crate::core::MultiTokenCore;
        use $crate::core::MultiTokenResolver;
        use $crate::{TokenId, TokenType};

        #[near_bindgen]
        impl MultiTokenCore for $contract {
            #[payable]
            fn mt_transfer(
                &mut self,
                receiver_id: AccountId,
                token_id: TokenId,
                amount: U128,
                memo: Option<String>,
            ) {
                self.$token.mt_transfer(receiver_id, token_id, amount, memo)
            }

            #[payable]
            fn mt_transfer_call(
                &mut self,
                receiver_id: AccountId,
                token_id: TokenId,
                amount: U128,
                memo: Option<String>,
                msg: String,
            ) -> PromiseOrValue<U128> {
                self.$token.mt_transfer_call(receiver_id, token_id, amount, memo, msg)
            }

            #[payable]
            fn mt_batch_transfer(
                &mut self,
                receiver_id: AccountId,
                token_id: Vec<TokenId>,
                amounts: Vec<U128>,
                memo: Option<String>,
            ) {
                self.$token.mt_batch_transfer(receiver_id, token_id, amounts, memo)
            }

            #[payable]
            fn mt_batch_transfer_call(
                &mut self,
                receiver_id: AccountId,
                token_ids: Vec<TokenId>,
                amounts: Vec<U128>,
                memo: Option<String>,
                msg: String,
            ) -> PromiseOrValue<Vec<U128>> {
                self.$token.mt_batch_transfer_call(receiver_id, token_ids, amounts, memo, msg)
            }

            fn balance_of(&self, owner_id: AccountId, token_id: TokenId) -> U128 {
                self.$token.balance_of(owner_id, token_id)
            }

            fn balance_of_batch(&self, owner_id: AccountId, token_ids: Vec<TokenId>) -> Vec<U128> {
                self.$token.balance_of_batch(owner_id, token_ids)
            }

            fn total_supply(&self, token_id: TokenId) -> U128 {
                self.$token.total_supply(token_id)
            }

            fn total_supply_batch(&self, token_ids: Vec<TokenId>) -> Vec<U128> {
                self.$token.total_supply_batch(token_ids)
            }
        }

        #[near_bindgen]
        impl MultiTokenResolver for $contract {
            #[private]
            fn mt_resolve_transfer(
                &mut self,
                sender_id: AccountId,
                receiver_id: AccountId,
                token_ids: Vec<TokenId>,
                amounts: Vec<U128>,
            ) -> Vec<U128> {
                self.$token.mt_resolve_transfer(sender_id, receiver_id, token_ids, amounts)
            }
        }
    };
}

#[macro_export]
macro_rules! impl_multi_token_core_with_minter {
    ($contract: ident, $token: ident) => {
        use $crate::core::MultiTokenMinter;
        use $crate::impl_multi_token_core;
        use $crate::metadata::MultiTokenMetadata;

        impl_multi_token_core!($contract, $token);

        #[near_bindgen]
        impl MultiTokenMinter for $contract {
            fn mint(
                &mut self,
                token_id: TokenId,
                token_type: TokenType,
                amount: Option<U128>,
                token_owner_id: ValidAccountId,
                token_metadata: Option<MultiTokenMetadata>,
            ) {
                self.$token.mint(token_id, token_type, amount, token_owner_id, token_metadata)
            }
        }
    };
}