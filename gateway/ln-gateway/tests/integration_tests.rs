//! Gateway integration test suite
//!
//! This crate contains integration tests for the gateway API
//! and business logic.
mod fixtures;

use std::sync::Arc;

use anyhow::bail;
use fedimint_core::config::FederationId;
use fedimint_core::Amount;
use fedimint_dummy_client::DummyClientExt;
use fedimint_testing::btc::BitcoinTest;
use fedimint_testing::federation::FederationTest;
use fedimint_testing::fixtures::TIMEOUT;
use fedimint_testing::gateway::GatewayTest;
use fedimint_wallet_client::{DepositState, WalletClientExt};
use futures::StreamExt;
use ln_gateway::rpc::rpc_client::GatewayRpcClient;
use ln_gateway::rpc::{BalancePayload, ConnectFedPayload, WithdrawPayload};

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_supports_connecting_multiple_federations() {
    let (_, rpc, fed1, fed2, _) = fixtures::fixtures().await;

    assert_eq!(rpc.get_info().await.unwrap().federations.len(), 0);

    let connection1 = fed1.connection_code();
    let info = rpc
        .connect_federation(ConnectFedPayload {
            connect: connection1.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(info.federation_id, connection1.id);

    let connection2 = fed2.connection_code();
    let info = rpc
        .connect_federation(ConnectFedPayload {
            connect: connection2.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(info.federation_id, connection2.id);
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_shows_info_about_all_connected_federations() {
    let (_, rpc, fed1, fed2, _) = fixtures::fixtures().await;

    assert_eq!(rpc.get_info().await.unwrap().federations.len(), 0);

    let id1 = fed1.connection_code().id;
    let id2 = fed2.connection_code().id;

    connect_federations(&rpc, &[fed1, fed2]).await.unwrap();

    let info = rpc.get_info().await.unwrap();

    assert_eq!(info.federations.len(), 2);
    assert!(info
        .federations
        .iter()
        .any(|info| info.federation_id == id1 && info.balance_msat == Amount::ZERO));
    assert!(info
        .federations
        .iter()
        .any(|info| info.federation_id == id2 && info.balance_msat == Amount::ZERO));
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_shows_balance_for_any_connected_federation() {
    let (gateway, rpc, fed1, fed2, _) = fixtures::fixtures().await;

    let id1 = fed1.connection_code().id;
    let id2 = fed2.connection_code().id;

    connect_federations(&rpc, &[fed1, fed2]).await.unwrap();

    let pre_balances = get_balances(&rpc, &[id1, id2]).await;

    send_msats_to_gateway(&gateway, id1, 5_000).await;
    send_msats_to_gateway(&gateway, id2, 1_000).await;

    let post_balances = get_balances(&rpc, &[id1, id2]).await;

    assert_eq!(pre_balances[0], 0);
    assert_eq!(pre_balances[1], 0);
    assert_eq!(post_balances[0], 5_000);
    assert_eq!(post_balances[1], 1_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_allows_deposit_to_any_connected_federation() {
    let (gateway, rpc, fed1, fed2, bitcoin) = fixtures::fixtures().await;

    let id1 = fed1.connection_code().id;
    let id2 = fed2.connection_code().id;

    connect_federations(&rpc, &[fed1, fed2]).await.unwrap();

    bitcoin.prepare_funding_wallet().await;

    deposit_and_wait_for_confirmation(&gateway, bitcoin.clone(), id1, 5_000_000)
        .await
        .unwrap();
    deposit_and_wait_for_confirmation(&gateway, bitcoin.clone(), id2, 1_000_000)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_allows_withdrawal_from_any_connected_federation() {
    let (gateway, rpc, fed1, fed2, bitcoin) = fixtures::fixtures().await;

    let id1 = fed1.connection_code().id;
    let id2 = fed2.connection_code().id;

    connect_federations(&rpc, &[fed1, fed2]).await.unwrap();

    bitcoin.prepare_funding_wallet().await;
    deposit_and_wait_for_confirmation(&gateway, bitcoin.clone(), id1, 5_000_000)
        .await
        .unwrap();
    deposit_and_wait_for_confirmation(&gateway, bitcoin.clone(), id2, 5_000_000)
        .await
        .unwrap();

    let address = bitcoin.get_new_address().await;

    withdraw(&rpc, id1, address.clone(), 1_000_000)
        .await
        .unwrap();
    withdraw(&rpc, id2, address, 2_000_000).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_supports_backup_of_any_connected_federation() -> anyhow::Result<()> {
    // todo: implement test case

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_supports_restore_of_any_connected_federation() -> anyhow::Result<()> {
    // todo: implement test case

    Ok(())
}

// Internal payments within a federation should not involve the gateway. See
// Issue #613: Federation facilitates internal payments w/o involving gateway
#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_pays_internal_invoice_within_a_connected_federation() -> anyhow::Result<()> {
    // todo: implement test case

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_pays_outgoing_invoice_to_generic_ln() -> anyhow::Result<()> {
    // todo: implement test case

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_pays_outgoing_invoice_between_federations_connected() -> anyhow::Result<()> {
    // todo: implement test case

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn gatewayd_intercepts_htlc_and_settles_to_connected_federation() -> anyhow::Result<()> {
    // todo: implement test case

    Ok(())
}

pub async fn connect_federations(
    rpc: &GatewayRpcClient,
    feds: &[FederationTest],
) -> anyhow::Result<()> {
    for fed in feds {
        let connect = fed.connection_code().to_string();
        rpc.connect_federation(ConnectFedPayload { connect })
            .await?;
    }
    Ok(())
}

async fn get_balances(
    rpc: &GatewayRpcClient,
    ids: impl IntoIterator<Item = &FederationId>,
) -> Vec<u64> {
    let mut balances = vec![];
    for id in ids.into_iter() {
        balances.push(
            rpc.get_balance(BalancePayload { federation_id: *id })
                .await
                .unwrap()
                .msats,
        )
    }

    balances
}

async fn send_msats_to_gateway(gateway: &GatewayTest, id: FederationId, msats: u64) {
    let client = gateway.select_client(id).await;
    let (_, outpoint) = client.print_money(Amount::from_msats(msats)).await.unwrap();
    client.receive_money(outpoint).await.unwrap();
}

async fn deposit_and_wait_for_confirmation(
    gateway: &GatewayTest,
    bitcoin: Arc<dyn BitcoinTest>,
    id: FederationId,
    sats: u64,
) -> anyhow::Result<()> {
    let valid_until = std::time::SystemTime::now() + TIMEOUT;
    let finality_delay = 10;

    let client = gateway.select_client(id).await;
    let (op, address) = client.get_deposit_address(valid_until).await.unwrap();
    let mut stream = client
        .subscribe_deposit_updates(op)
        .await
        .unwrap()
        .into_stream();

    bitcoin
        .send_and_mine_block(&address, bitcoin::Amount::from_sat(sats))
        .await;

    while let Some(state) = stream.next().await {
        match state {
            DepositState::Failed(e) => {
                bail!("failed to receive deposit: {}", e)
            }
            DepositState::Confirmed => break,
            _ => bitcoin.mine_blocks(finality_delay).await,
        }
    }

    Ok(())
}

async fn withdraw(
    rpc: &GatewayRpcClient,
    id: FederationId,
    address: bitcoin::Address,
    sats: u64,
) -> anyhow::Result<()> {
    rpc.withdraw(WithdrawPayload {
        federation_id: id,
        amount: bitcoin::Amount::from_sat(sats),
        address,
    })
    .await?;

    Ok(())
}
