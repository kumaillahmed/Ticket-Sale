use std::{sync::Arc, thread, time::Duration};

use eyre::Result;
use futures::executor::block_on;
use ticket_sale_tests::TestCtxBuilder;
use uuid::Uuid;

#[tokio::test]
#[ntest::timeout(20_000)] // Test timeout in ms
async fn test_scale_server_response() -> Result<()> {
    // Create 200000 tickets
    let ctx = TestCtxBuilder::from_env()?.build().await?;
    let api = Arc::new(ctx);

    // Scale server from requests from 100 admins
    let mut threads = Vec::new();
    for i in 0..100 {
        let api = Arc::clone(&api);
        threads.push(thread::spawn(move || {
            let user = api.api.create_user_session(None);
            let response = block_on(user.api.post_num_servers(i))
                .unwrap()
                .result
                .unwrap();
            assert_eq!(response, i);
        }));
    }

    for thread_running in threads {
        while !thread_running.is_finished() {
            thread::sleep(Duration::from_millis(25));
        }
    }

    Arc::into_inner(api).unwrap().finish().await;
    Ok(())
}

#[tokio::test]
#[ntest::timeout(20_000)] // Test timeout in ms
async fn test_scale_same_number_of_servers() -> Result<()> {
    let ctx = TestCtxBuilder::from_env()?.build().await?;
    let api = Arc::new(ctx);

    // Scale to 10 servers initially
    let initial_num_servers = 10;
    api.api.post_num_servers(initial_num_servers).await?;

    // Validate the number of servers
    assert_eq!(
        api.api.get_num_servers().await?.result?,
        initial_num_servers,
        "Initially scaled to {} servers, the number of servers must be {}.",
        initial_num_servers, initial_num_servers
    );

    // Scale to the same number of servers
    api.api.post_num_servers(initial_num_servers).await?;

    // Validate the number of servers remains the same
    assert_eq!(
        api.api.get_num_servers().await?.result?,
        initial_num_servers,
        "After scaling to the same number, the number of servers must still be {}.",
        initial_num_servers
    );

    Arc::into_inner(api).unwrap().finish().await;
    Ok(())
}

#[tokio::test]
#[ntest::timeout(20_000)] // Test timeout in ms
async fn test_scale_down_servers() -> Result<()> {
    let ctx = TestCtxBuilder::from_env()?.build().await?;
    let api = Arc::new(ctx);

    // Scale to 10 servers initially
    let initial_num_servers = 10;
    api.api.post_num_servers(initial_num_servers).await?;

    // Validate the number of servers
    assert_eq!(
        api.api.get_num_servers().await?.result?,
        initial_num_servers,
        "Initially scaled to {} servers, the number of servers must be {}.",
        initial_num_servers, initial_num_servers
    );

    // Scale down to 5 servers
    let reduced_num_servers = 5;
    api.api.post_num_servers(reduced_num_servers).await?;

    // Validate the number of servers
    assert_eq!(
        api.api.get_num_servers().await?.result?,
        reduced_num_servers,
        "After scaling down to {} servers, the number of servers must be {}.",
        reduced_num_servers, reduced_num_servers
    );

    Arc::into_inner(api).unwrap().finish().await;
    Ok(())
}

#[tokio::test]
#[ntest::timeout(20_000)] // Test timeout in ms
async fn test_scale_to_zero_servers() -> Result<()> {
    let ctx = TestCtxBuilder::from_env()?.build().await?;
    let api = Arc::new(ctx);

    // Scale to 5 servers initially
    let initial_num_servers = 5;
    api.api.post_num_servers(initial_num_servers).await?;

    // Validate the number of servers
    assert_eq!(
        api.api.get_num_servers().await?.result?,
        initial_num_servers,
        "Initially scaled to {} servers, the number of servers must be {}.",
        initial_num_servers, initial_num_servers
    );

    // Scale down to 0 servers
    let reduced_num_servers = 0;
    api.api.post_num_servers(reduced_num_servers).await?;

    // Validate the number of servers
    assert_eq!(
        api.api.get_num_servers().await?.result?,
        reduced_num_servers,
        "After scaling down to {} servers, the number of servers must be {}.",
        reduced_num_servers, reduced_num_servers
    );

    Arc::into_inner(api).unwrap().finish().await;
    Ok(())
}

#[tokio::test]
#[ntest::timeout(20_000)] // Test timeout in ms
async fn test_scale_to_max_servers() -> Result<()> {
    let ctx = TestCtxBuilder::from_env()?.build().await?;
    let api = Arc::new(ctx);

    // Scale to a very large number of servers
    let max_num_servers = 1000; // Example value, adjust as needed
    api.api.post_num_servers(max_num_servers).await?;

    // Validate the number of servers
    assert_eq!(
        api.api.get_num_servers().await?.result?,
        max_num_servers,
        "After scaling to {} servers, the number of servers must be {}.",
        max_num_servers, max_num_servers
    );

    Arc::into_inner(api).unwrap().finish().await;
    Ok(())
}
