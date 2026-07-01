use crate::Result;
use faro_capture::{AdapterError, BrowserEvent, EventIngestor};
use faro_store::Store;

pub(super) fn ingest_or_ignore_unknown(
    ingestor: &mut EventIngestor,
    store: &Store,
    event: BrowserEvent,
) -> Result<bool> {
    match ingestor.ingest(store, event) {
        Ok(_) => Ok(true),
        Err(AdapterError::UnknownBrowserRequest(_)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}
