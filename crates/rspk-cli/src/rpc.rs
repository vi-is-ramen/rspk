//! Bridge between CLI `App` and the JSON-RPC server.

use crate::app::App;
use rspk_rpc::Server;

/// Starts the JSON-RPC 2.0 server using the application's registry.
///
/// Takes ownership of the registry (cheap, via `Arc` internally) and
/// runs until stdin is closed.
pub async fn serve(app: &App) -> anyhow::Result<()>
{
    // The App owns the registry; we need to clone it out. Since
    // ManagerRegistry is not Clone, we re-discover it — or expose
    // a take() method. For simplicity we re-discover: the cost is
    // amortized across the whole RPC session.
    //
    // In production you'd add a `registry()` method on App that
    // returns a shared reference; we do that here.
    let registry = app.take_registry()?;
    let server = Server::new(registry, app.ctx.clone());
    server.run().await?;
    Ok(())
}
