use std::{sync::Arc, thread, time::Duration};

use eyre::Result;
use futures::executor::block_on;
use ticket_sale_tests::TestCtxBuilder;

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
