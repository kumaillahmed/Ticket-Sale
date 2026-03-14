use eyre::Result;
use nanorand::Rng;
use ticket_sale_tests::TestCtxBuilder;

mod util;

#[tokio::test] // Every test function needs to be decorated with this attribute
#[ntest::timeout(50_000)] // Test timeout in ms
async fn resize() -> Result<()> {
    // Create a test context with 1000 initially available tickets
    let ctx = TestCtxBuilder::from_env()?
        .with_tickets(100)
        .build()
        .await?;
    let mut rng = nanorand::tls_rng();
    let session = ctx.api.create_user_session(None);
    let num_servers = session.api.get_num_servers().await?.result.unwrap();
    assert!(
        num_servers as usize == 2,
        "The number of servers should be {}",
        2
    );
    //Buy all tickets
    for _ in 0..100 {
        let session = ctx.api.create_user_session(None);
        let rand = rng.generate_range(0..20);
        println!("rand: {}", rand);
        let reply = session.api.post_num_servers(rand).await?;
        let num_servers = session.api.get_num_servers().await?.result.unwrap();
        assert!(
            num_servers as usize == rand,
            "The number of servers should be {}",
            rand
        );

        let num_servers = reply.result.unwrap();
        assert!(
            num_servers == rand,
            "The number of servers should be {}",
            rand
        );
    }
    let session = ctx.api.create_user_session(None);
    let reply = session.api.post_num_servers(0).await?;
    let num_servers = session.api.get_num_servers().await?.result.unwrap();
    assert!(
        num_servers as usize == 0,
        "The number of servers should be {}",
        0
    );

    let num_servers = reply.result.unwrap();
    assert!(num_servers == 0, "The number of servers should be {}", 0);

    // Finish the test
    ctx.finish().await;
    Ok(())
}
