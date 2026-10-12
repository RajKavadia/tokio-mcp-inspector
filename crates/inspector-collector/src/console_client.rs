//! External Console gRPC collector (Experiment A).
//!
//! Connects as a *separate process* via `console-api` tonic client,
//! streams a bounded window of `WatchUpdates`, and normalizes into
//! versioned [`inspector_core`] types. Unsupported fields stay `None`
//! and are recorded in `Completeness.missing_fields` — never guessed.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use console_api::instrument::instrument_client::InstrumentClient;
use console_api::instrument::InstrumentRequest;
use inspector_core::{
    Completeness, CoreError, Observation, ResourceRecord, RuntimeOverview, Snapshot, SourceId,
    TaskRecord,
};

use super::status::{CollectorStatus, ConnectionConfig};

pub struct ConsoleCollector {
    pub config: ConnectionConfig,
    started: Instant,
}

impl ConsoleCollector {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            started: Instant::now(),
        }
    }

    pub fn status(&self) -> CollectorStatus {
        CollectorStatus::Connected {
            addr: self.config.addr.clone(),
        }
    }

    fn endpoint(&self) -> String {
        if self.config.addr.starts_with("http") {
            self.config.addr.clone()
        } else {
            format!("http://{}", self.config.addr)
        }
    }

    /// Capture one bounded snapshot: connect + collect updates for a short window.
    pub async fn capture_snapshot(
        &self,
        service_name: Option<String>,
    ) -> Result<Observation<Snapshot>, CoreError> {
        let window = self.config.collection_timeout();
        let max_tasks = self.config.max_tasks;
        let max_resources = self.config.max_resources;

        let mut client = tokio::time::timeout(
            Duration::from_millis(self.config.connect_timeout_ms),
            InstrumentClient::connect(self.endpoint()),
        )
        .await
        .map_err(|_| CoreError::Timeout {
            after_ms: self.config.connect_timeout_ms,
            op: format!("connect {}", self.config.addr),
        })
        .and_then(|r| r.map_err(|e| CoreError::Unavailable(e.to_string())))?;

        let mut stream = tokio::time::timeout(window, client.watch_updates(InstrumentRequest {}))
            .await
            .map_err(|_| CoreError::Timeout {
                after_ms: window.as_millis() as u64,
                op: "watch_updates".into(),
            })
            .and_then(|r| r.map_err(|e| CoreError::Unavailable(e.to_string())))?
            .into_inner();

        let deadline = Instant::now() + window;
        let mut tasks: HashMap<u64, TaskRecord> = HashMap::new();
        let mut resources: HashMap<u64, ResourceRecord> = HashMap::new();
        let mut warnings = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        let mut tasks_truncated = false;
        let mut dropped_events: u64 = 0;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let msg = match tokio::time::timeout(remaining, stream.message()).await {
                Ok(Ok(Some(u))) => u,
                Ok(Ok(None)) => break, // server closed stream
                Ok(Err(e)) => return Err(CoreError::Unavailable(e.to_string())),
                Err(_) => break, // window elapsed: normal end of bounded capture
            };

            if let Some(tu) = msg.task_update {
                dropped_events += tu.dropped_events;
                for t in tu.new_tasks {
                    let id = t.id.map(|i| i.id).unwrap_or(0);
                    let name = task_name(&t.fields);
                    if name.is_none() && !missing.iter().any(|m| m == "task.name") {
                        missing.push("task.name".to_string());
                    }
                    let location = t.location.as_ref().and_then(|l| {
                        l.file
                            .clone()
                            .map(|f| format!("{f}:{}", l.line.unwrap_or(0)))
                    });
                    tasks.entry(id).or_insert(TaskRecord {
                        task_id: id,
                        name,
                        polls: None, // filled by stats_update below
                        wakes: None,
                        self_wakes: None,
                        busy_ns: None,
                        scheduled_ns: None,
                        state: None,
                        location,
                    });
                    if tasks.len() >= max_tasks {
                        tasks_truncated = true;
                    }
                }
                for (id, stats) in tu.stats_update {
                    let e = tasks.entry(id).or_insert(TaskRecord {
                        task_id: id,
                        name: None,
                        polls: None,
                        wakes: None,
                        self_wakes: None,
                        busy_ns: None,
                        scheduled_ns: None,
                        state: None,
                        location: None,
                    });
                    e.polls = Some(stats.poll_stats.as_ref().map(|p| p.polls).unwrap_or(0));
                    e.wakes = Some(stats.wakes);
                    e.self_wakes = Some(stats.self_wakes);
                    e.scheduled_ns = stats
                        .scheduled_time
                        .map(|d| (d.seconds.max(0) as u64) * 1_000_000_000 + d.nanos.max(0) as u64);
                    // busy time lives inside PollStats histograms in newer APIs;
                    // not decoded here -> stays None, recorded as missing.
                    if e.busy_ns.is_none() && !missing.iter().any(|m| m == "task.busy_ns") {
                        missing.push("task.busy_ns".to_string());
                    }
                    if stats.dropped_at.is_some() {
                        e.state = Some("done".to_string());
                    }
                }
            }

            if let Some(ru) = msg.resource_update {
                dropped_events += ru.dropped_events;
                for r in ru.new_resources {
                    let id = r.id.map(|i| i.id).unwrap_or(0);
                    resources.entry(id).or_insert(ResourceRecord {
                        resource_id: id,
                        concrete_type: if r.concrete_type.is_empty() {
                            None
                        } else {
                            Some(r.concrete_type)
                        },
                        kind: resource_kind(&r.kind),
                        attributes: Vec::new(),
                        location: r.location.as_ref().and_then(|l| l.file.clone()),
                    });
                }
                for (id, stats) in ru.stats_update {
                    let e = resources.entry(id).or_insert(ResourceRecord {
                        resource_id: id,
                        concrete_type: None,
                        kind: None,
                        attributes: Vec::new(),
                        location: None,
                    });
                    e.attributes = decode_attributes(&stats.attributes);
                }
            }

            // Bound the window: stop early once we have a useful sample.
            if tasks.len() >= max_tasks && resources.len() >= max_resources {
                break;
            }
        }

        if dropped_events > 0 {
            warnings.push(format!(
                "{dropped_events} events dropped by target buffer; snapshot partial"
            ));
        }
        if tasks_truncated {
            warnings.push(format!("task list truncated at {max_tasks}"));
        }

        let completeness = if missing.is_empty() && warnings.is_empty() {
            Completeness::complete()
        } else {
            Completeness::partial(missing, warnings)
        };

        let mut task_vec: Vec<TaskRecord> = tasks.into_values().collect();
        task_vec.sort_by_key(|t| t.task_id);
        task_vec.truncate(max_tasks);
        let mut res_vec: Vec<ResourceRecord> = resources.into_values().collect();
        res_vec.sort_by_key(|r| r.resource_id);
        res_vec.truncate(max_resources);

        let overview = RuntimeOverview {
            service_name: service_name.clone(),
            console_addr: self.config.addr.clone(),
            active_tasks: Some(task_vec.len() as u64),
            completed_tasks: None, // not tracked reliably here -> None, not 0
            tasks_truncated,
            // App-level metric names are merged by metrics_adapter, not invented here.
            metric_names: Vec::new(),
        };

        let snap = Snapshot {
            overview,
            tasks: task_vec,
            resources: res_vec,
            metrics: Vec::new(),
        };

        Ok(Observation::new(
            SourceId {
                console_addr: self.config.addr.clone(),
                service_name,
            },
            Some(window.as_millis() as u64),
            self.started.elapsed().as_millis() as u64,
            snap,
            completeness,
        ))
    }
}

fn resource_kind(kind: &Option<console_api::resources::resource::Kind>) -> Option<String> {
    use console_api::resources::resource::kind::Kind;
    let k = kind.as_ref()?.kind.as_ref()?;
    match k {
        Kind::Known(v) => Some(format!("known({v})")),
        Kind::Other(s) => Some(format!("other({s})")),
    }
}

/// Decode resource state attributes (e.g. semaphore permits, timer durations)
/// into plain string pairs. Unknown shapes are skipped, never fabricated.
fn decode_attributes(attrs: &[console_api::Attribute]) -> Vec<(String, String)> {
    use console_api::field::{Name, Value};
    let mut out = Vec::new();
    for a in attrs {
        let Some(field) = a.field.as_ref() else {
            continue;
        };
        let key = match &field.name {
            Some(Name::StrName(s)) => s.clone(),
            _ => continue,
        };
        let mut value = match &field.value {
            Some(Value::BoolVal(v)) => v.to_string(),
            Some(Value::StrVal(v)) => v.clone(),
            Some(Value::U64Val(v)) => v.to_string(),
            Some(Value::I64Val(v)) => v.to_string(),
            Some(Value::DebugVal(v)) => v.clone(),
            None => continue,
        };
        if let Some(unit) = a.unit.as_ref() {
            value.push(' ');
            value.push_str(unit);
        }
        out.push((key, value));
    }
    out
}

/// Task names arrive as `tracing` span fields (`name` / `task.name`).
fn task_name(fields: &[console_api::Field]) -> Option<String> {
    use console_api::field::{Name, Value};
    for f in fields {
        let key = match &f.name {
            Some(Name::StrName(s)) => s.clone(),
            _ => continue,
        };
        if key == "name" || key == "task.name" {
            if let Some(Value::StrVal(s)) = &f.value {
                return Some(s.clone());
            }
        }
    }
    None
}
