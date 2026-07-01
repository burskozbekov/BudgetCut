//! BudgetCut sync server entry point. Binds `BUDGETCUT_BIND` (default
//! 127.0.0.1:8787) and serves the Axum app. State is in-memory; set
//! `BUDGETCUT_JWT_SECRET` in production.

#[tokio::main]
async fn main() {
    let state = budgetcut_server::AppState::new();
    let app = budgetcut_server::app(state);

    let addr = std::env::var("BUDGETCUT_BIND").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    println!("BudgetCut sync server listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
}
