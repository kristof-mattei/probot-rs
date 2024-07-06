use std::env;
use std::sync::Arc;
use std::time::Duration;
fn main() -> Result<(), color_eyre::Report> {
    // initialize the runtime
    let rt = tokio::runtime::Runtime::new().unwrap();

    // start service
    let result: Result<(), color_eyre::Report> = rt.block_on(start_tasks());

    result
}

async fn start_tasks() -> Result<(), color_eyre::Report> {
    let token = CancellationToken::new();

    let mut tasks = tokio::task::JoinSet::new();

    {
        let token = token.clone();

        tasks.spawn(async move {
            match server_forever(bind_to, router, token.clone()).await {
                Err(err) => {
                    event!(Level::ERROR, ?err, "Server died");
                },
                Ok(()) => {
                    event!(Level::INFO, "Server shut down");
                },
            }
        });
    }

    // now we wait forever for either
    // * SIGTERM
    // * ctrl + c (SIGINT)
    // * a message on the shutdown channel, sent either by the server task or
    // another task when they complete (which means they failed)
    tokio::select! {
        _ = signal_handlers::wait_for_sigint() => {
            // we completed because ...
            event!(Level::WARN, message = "CTRL+C detected, stopping all tasks");
        },
        _ = tasks.join_next() => {},
        _ = signal_handlers::wait_for_sigterm() => {
            // we completed because ...
            event!(Level::WARN, message = "Sigterm detected, stopping all tasks");
        },
        () = token.cancelled() => {
            event!(Level::WARN, "Underlying task stopped, stopping all others tasks");
        },
    };

    // backup, in case we forgot a dropguard somewhere
    token.cancel();

    // wait for the task that holds the server to exit gracefully
    // it listens to shutdown_send
    if timeout(Duration::from_millis(10000), tasks.shutdown())
        .await
        .is_err()
    {
        event!(Level::ERROR, "Tasks didn't stop within allotted time!");
    }

    {
        (statistics.read().await).log_totals::<()>(&[]);
    }

    event!(Level::INFO, "Goodbye");

    Ok(())
}
