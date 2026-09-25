use super::memory::MemoryEventStore;
use super::{DomainKey, EventStore, StoredDecision, StoredEvent, StoredOutcome};
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct JsonFileEventStore {
    path: PathBuf,
    inner: MemoryEventStore,
    fatal_error: Option<String>,
}

impl JsonFileEventStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let inner = if path.exists() {
            let bytes = fs::read(&path)
                .map_err(|e| format!("failed to read event store {}: {e}", path.display()))?;
            if bytes.is_empty() {
                MemoryEventStore::new()
            } else {
                serde_json::from_slice(&bytes)
                    .map_err(|e| format!("failed to decode event store {}: {e}", path.display()))?
            }
        } else {
            MemoryEventStore::new()
        };

        Ok(Self {
            path,
            inner,
            fatal_error: None,
        })
    }

    fn temp_path(&self) -> PathBuf {
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("r2r-store.json");
        self.path.with_file_name(format!(".{file_name}.tmp"))
    }

    fn persist(&mut self) {
        if self.fatal_error.is_some() {
            return;
        }

        if let Err(error) = self.persist_inner() {
            self.fatal_error = Some(error);
        }
    }

    fn persist_inner(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("failed to create store directory {}: {e}", parent.display())
                })?;
            }
        }

        let encoded = serde_json::to_vec_pretty(&self.inner)
            .map_err(|e| format!("failed to encode event store: {e}"))?;
        let temp_path = self.temp_path();
        fs::write(&temp_path, encoded).map_err(|e| {
            format!(
                "failed to write temporary store {}: {e}",
                temp_path.display()
            )
        })?;
        fs::rename(&temp_path, &self.path).map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            format!(
                "failed to atomically replace event store {}: {e}",
                self.path.display()
            )
        })?;
        Ok(())
    }

    #[cfg(test)]
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl EventStore for JsonFileEventStore {
    fn health(&self) -> Result<(), String> {
        match &self.fatal_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn current_state_version(&self, domain: &DomainKey) -> String {
        self.inner.current_state_version(domain)
    }

    fn advance_state_version(&mut self, domain: &DomainKey) -> String {
        self.inner.advance_state_version(domain)
    }

    fn next_event_id(&mut self) -> String {
        self.inner.next_event_id()
    }

    fn record_event(&mut self, event: StoredEvent) {
        self.inner.record_event(event);
        self.persist();
    }

    fn event(&self, event_id: &str) -> Option<StoredEvent> {
        self.inner.event(event_id)
    }

    fn events_for_domain(&self, domain: &DomainKey) -> Vec<StoredEvent> {
        self.inner.events_for_domain(domain)
    }

    fn all_events(&self) -> Vec<StoredEvent> {
        self.inner.all_events()
    }

    fn next_decision_id(&mut self) -> String {
        self.inner.next_decision_id()
    }

    fn record_decision(&mut self, decision: StoredDecision) {
        self.inner.record_decision(decision);
        self.persist();
    }

    fn decision(&self, decision_id: &str) -> Option<StoredDecision> {
        self.inner.decision(decision_id)
    }

    fn next_outcome_id(&mut self) -> String {
        self.inner.next_outcome_id()
    }

    fn record_outcome(&mut self, outcome: StoredOutcome) {
        self.inner.record_outcome(outcome);
        self.persist();
    }

    fn outcome(&self, outcome_id: &str) -> Option<StoredOutcome> {
        self.inner.outcome(outcome_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ReplayObservation, StoredEvent};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_store_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "r2r-jev-{label}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    #[test]
    fn persists_and_reopens_snapshot() {
        let path = temp_store_path("reopen");
        let domain = DomainKey::new("agent:a", "repo:alpha");

        {
            let mut store = JsonFileEventStore::open(&path).expect("open");
            assert_eq!(store.path(), path.as_path());
            assert_eq!(store.advance_state_version(&domain), "state-000001");
            let event_id = store.next_event_id();
            store.record_event(StoredEvent {
                event_id: event_id.clone(),
                kernel_event_id: "ev-0001".to_string(),
                domain: domain.clone(),
                action: "read_file".to_string(),
                state_version: "state-000001".to_string(),
                relation_transitions: vec![
                    "authorization-0001:Authorization:Active -> Suspended".to_string()
                ],
                provenance: vec!["demo".to_string()],
                replay_observation: Some(ReplayObservation {
                    provider: "fixture".to_string(),
                    task: "demo".to_string(),
                    action: "read_file".to_string(),
                    intent: "demo".to_string(),
                    beyond_scope_ppm: 940_000,
                    destructive_ppm: 100_000,
                    source_reliability_ppm: 850_000,
                    independent_corroborators: 0,
                    now_vtick: 1,
                    expires_vtick: 11,
                    admission_policy_version: "0.1".to_string(),
                }),
            });
            assert!(store.health().is_ok());
        }

        let mut reopened = JsonFileEventStore::open(&path).expect("reopen");
        assert_eq!(reopened.current_state_version(&domain), "state-000001");
        assert!(reopened.event("event-000001").is_some());
        assert_eq!(reopened.next_event_id(), "event-000002");

        let _ = fs::remove_file(path);
    }
}
