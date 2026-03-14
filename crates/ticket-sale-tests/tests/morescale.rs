use std::{thread, time::Duration};

use eyre::{Ok, Result};
use ticket_sale_tests::{RequestOptions, TestCtxBuilder};
use util::scale_to;

mod util;

#[tokio::test]
#[ntest::timeout(20_000)] // Test timeout in ms
async fn test_num_tickets_on_scale() -> Result<()> {
    let ctx = TestCtxBuilder::from_env()?
        .with_tickets(100)
        .with_estimator_roundtrip_time(1)
        .build()
        .await?;

    scale_to(&ctx, 5).await?;

    thread::sleep(Duration::from_secs(1));

    let num_available_tickets = ctx
        .api
        .get_available_tickets(&RequestOptions::default())
        .await?
        .result
        .unwrap();

    assert_eq!(num_available_tickets, 100, "Expected 100 available tickets");

    scale_to(&ctx, 1).await?;

    thread::sleep(Duration::from_secs(1));

    let num_available_tickets = ctx
        .api
        .get_available_tickets(&RequestOptions::default())
        .await?
        .result
        .unwrap();

    assert_eq!(num_available_tickets, 100, "Expected 100 available tickets");

    ctx.finish().await;
    Ok(())
}

#[tokio::test]
#[ntest::timeout(20_000)] // Test timeout in ms
async fn test_num_servers_on_scale_with_busy_servers() -> Result<()> {
    let ctx = TestCtxBuilder::from_env()?.build().await?;

    scale_to(&ctx, 5).await?;

    let server_id = ctx.api.get_servers().await?.result?.pop().unwrap();
    ctx.api
        .reserve_ticket(&RequestOptions {
            server_id: Some(server_id),
            customer_id: None,
        })
        .await?;

    scale_to(&ctx, 2).await?;

    ctx.finish().await;
    Ok(())
}
