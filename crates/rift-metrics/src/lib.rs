//! Lightweight in-process metrics collection.
//!
//! This crate intentionally avoids external dependencies or exporters. It stores
//! counters, gauges, and histograms in memory and can render a simple text
//! representation for debugging or log capture.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

static ENABLED: AtomicBool = AtomicBool::new(true);
static STORE: Lazy<Mutex<MetricsStore>> = Lazy::new(|| Mutex::new(MetricsStore::default()));

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MetricsStore {
    /// Monotonic counters.
    pub counters: HashMap<String, u64>,
    /// Last-set gauge values.
    pub gauges: HashMap<String, f64>,
    /// Aggregated histograms.
    pub histograms: HashMap<String, Histogram>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Histogram {
    /// Number of observations.
    pub count: u64,
    /// Sum of all observations.
    pub sum: f64,
    /// Minimum observed value.
    pub min: f64,
    /// Maximum observed value.
    pub max: f64,
}

impl Histogram {
    /// Record a new observation, updating min/max/sum/count.
    pub fn observe(&mut self, value: f64) {
        if self.count == 0 {
            self.min = value;
            self.max = value;
        } else {
            if value < self.min {
                self.min = value;
            }
            if value > self.max {
                self.max = value;
            }
        }
        self.count += 1;
        self.sum += value;
    }
}

/// Enable or disable metrics collection globally.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Increment a counter by 1.
pub fn inc_counter(name: &str, labels: &[(&str, &str)]) {
    add_counter(name, labels, 1);
}

/// Add a value to a counter (saturating on overflow).
pub fn add_counter(name: &str, labels: &[(&str, &str)], value: u64) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let key = format_key(name, labels);
    let mut store = STORE.lock();
    let entry = store.counters.entry(key).or_insert(0);
    *entry = entry.saturating_add(value);
}

/// Set a gauge to an explicit value.
pub fn set_gauge(name: &str, labels: &[(&str, &str)], value: f64) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let key = format_key(name, labels);
    let mut store = STORE.lock();
    store.gauges.insert(key, value);
}

/// Observe a histogram value (stored as count/sum/min/max).
pub fn observe_histogram(name: &str, labels: &[(&str, &str)], value: f64) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let key = format_key(name, labels);
    let mut store = STORE.lock();
    store
        .histograms
        .entry(key)
        .or_default()
        .observe(value);
}

/// Snapshot the current metrics into an owned struct.
pub fn snapshot() -> MetricsStore {
    STORE.lock().clone()
}

/// Render metrics in a simple text format.
pub fn render_text() -> String {
    let store = snapshot();
    let mut lines = Vec::new();
    for (key, value) in store.counters {
        lines.push(format!("{key} {value}"));
    }
    for (key, value) in store.gauges {
        lines.push(format!("{key} {:.3}", value));
    }
    for (key, hist) in store.histograms {
        let avg = if hist.count > 0 {
            hist.sum / hist.count as f64
        } else {
            0.0
        };
        lines.push(format!(
            "{key} count={} avg={:.3} min={:.3} max={:.3}",
            hist.count, avg, hist.min, hist.max
        ));
    }
    lines.join("\n")
}

/// Format metric keys with optional labels.
fn format_key(name: &str, labels: &[(&str, &str)]) -> String {
    if labels.is_empty() {
        return name.to_string();
    }
    let mut parts = Vec::with_capacity(labels.len());
    for (k, v) in labels {
        parts.push(format!("{k}={v}"));
    }
    format!("{name}{{{}}}", parts.join(","))
}
