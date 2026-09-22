use crate::StoreClient;

/// True while a Heroic client process exists. Heroic snapshots GamesConfig at
/// startup and flushes that snapshot on settings save, so an Apply written
/// while it runs is dropped by the next save — callers stop it first.
/// Detection lives on `StoreClient`, so this toast and the Apply guard share
/// one predicate and can never disagree.
pub fn heroic_running() -> bool {
    StoreClient::Heroic.running()
}
