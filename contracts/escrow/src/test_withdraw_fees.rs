#![cfg(test)]

use crate::{ContractError, Escrow, EscrowClient, Payee};
use soroban_sdk::{testutils::Address as _, testutils::Ledger, token, Address, Env, IntoVal, Vec};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    (
        env,
        admin,
        seller,
        buyer,
        resolver,
        token_address,
        fee_collector,
    )
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

#[test]
fn test_withdraw_fees_after_multiple_escrows() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);

    mint_tokens(&env, &token, &buyer, 3000);

    // Complete 3 escrows that each accrue 1% fees via dispute release.
    for _ in 0..3 {
        let mut payees_72 = Vec::new(&env);
        payees_72.push_back(Payee {
            address: seller.clone(),
            bps: 10_000,
        });
        let id = client.create_escrow(
            &payees_72.into_val(&env),
            &None::<Address>,
            &resolver,
            &token,
            &1000_i128,
            &100_u32,
            &0_u32,
            &3600_u64,
            &None::<soroban_sdk::String>,
        );
        client.fund_escrow(&id, &buyer);
        client.mark_shipped(
            &seller,
            &id,
            &soroban_sdk::String::from_str(&env, "TRACK-WITHDRAW-1"),
        );
        client.raise_dispute(
            &buyer,
            &id,
            &soroban_sdk::Symbol::new(&env, "release"),
            &soroban_sdk::String::from_str(&env, "ok"),
            &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
        );
        client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Release);
        env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
        client.finalize_dispute(&resolver, &id);
    }

    // Total fees: 10 * 3 = 30 — all go directly to fee_collector
    let fee_collector_balance = token::Client::new(&env, &token).balance(&fee_collector);
    assert_eq!(fee_collector_balance, 30);
    assert_eq!(token::Client::new(&env, &token).balance(&contract_id), 0);

    // withdraw_fees is not used in the direct-to-collector model
    let to = Address::generate(&env);
    let result = client.try_withdraw_fees(&admin, &token, &to, &30);
    assert!(matches!(
        result,
        Err(Ok(ContractError::InsufficientBalance))
    ));
}

#[test]
fn test_withdraw_fees_multiple_tokens() {
    let (env, admin, seller, buyer, resolver, token_a, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);

    // Register a second token
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b)
        .address();

    // Accrue fees for Token A (1000 amount, 1% fee = 10)
    mint_tokens(&env, &token_a, &buyer, 1000);
    let mut payees_71 = Vec::new(&env);
    payees_71.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let id_a = client.create_escrow(
        &payees_71.into_val(&env),
        &None::<Address>,
        &resolver,
        &token_a,
        &1000_i128,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<soroban_sdk::String>,
    );
    client.fund_escrow(&id_a, &buyer);
    client.mark_shipped(
        &seller,
        &id_a,
        &soroban_sdk::String::from_str(&env, "TRACK-WITHDRAW-A"),
    );
    client.raise_dispute(
        &buyer,
        &id_a,
        &soroban_sdk::Symbol::new(&env, "release"),
        &soroban_sdk::String::from_str(&env, "ok"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );
    client.resolve_dispute(&resolver, &id_a, &crate::ResolutionType::Release);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &id_a);

    // Accrue fees for Token B (2000 amount, 2% fee = 40)
    mint_tokens(&env, &token_b, &buyer, 2000);
    let mut payees_70 = Vec::new(&env);
    payees_70.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let id_b = client.create_escrow(
        &payees_70.into_val(&env),
        &None::<Address>,
        &resolver,
        &token_b,
        &2000_i128,
        &200_u32,
        &0_u32,
        &3600_u64,
        &None::<soroban_sdk::String>,
    );
    client.fund_escrow(&id_b, &buyer);
    client.mark_shipped(
        &seller,
        &id_b,
        &soroban_sdk::String::from_str(&env, "TRACK-WITHDRAW-B"),
    );
    client.raise_dispute(
        &buyer,
        &id_b,
        &soroban_sdk::Symbol::new(&env, "release"),
        &soroban_sdk::String::from_str(&env, "ok"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );
    client.resolve_dispute(&resolver, &id_b, &crate::ResolutionType::Release);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &id_b);

    // Protocol fees go directly to fee_collector
    assert_eq!(
        token::Client::new(&env, &token_a).balance(&fee_collector),
        10
    );
    assert_eq!(
        token::Client::new(&env, &token_b).balance(&fee_collector),
        40
    );
    assert_eq!(token::Client::new(&env, &token_a).balance(&contract_id), 0);
    assert_eq!(token::Client::new(&env, &token_b).balance(&contract_id), 0);
}

#[test]
fn test_withdraw_fees_zero_amount() {
    let (env, admin, _seller, _buyer, _resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);

    let to = Address::generate(&env);

    let result = client.try_withdraw_fees(&admin, &token, &to, &0);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAmount))));
}
