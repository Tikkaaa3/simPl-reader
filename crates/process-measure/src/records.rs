//! Versioned, machine-readable run artifacts.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: &str = "process-measure/v1";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiError {
    pub api: String,
    pub os_code: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Metric<T> {
    Valid { value: T },
    Unavailable { error: ApiError },
    Unsupported { reason: String },
    TerminalUnavailable { reason: String },
}

impl<T> Metric<T> {
    pub fn valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CpuDerivedRecord {
    pub baseline_sequence: Option<u64>,
    pub spans_invalid_observation: bool,
    pub delta_user_100ns: Option<u64>,
    pub delta_kernel_100ns: Option<u64>,
    pub elapsed_qpc_ticks: Option<u64>,
    pub elapsed_100ns: Option<u64>,
    pub cpu_percent_one_logical: Option<f64>,
    pub cpu_percent_machine: Option<f64>,
    pub unavailable_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HelperSnapshotRecord {
    Complete { pids: Vec<u32>, assigned: u32 },
    Truncated { assigned: u32, limit: usize },
    Failed { error: ApiError },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SampleRecord {
    pub schema: String,
    pub run_id: String,
    pub sequence: u64,
    pub scheduled_qpc: u64,
    pub sample_start_qpc: u64,
    pub cpu_observation_qpc: u64,
    pub sample_end_qpc: u64,
    pub schedule_slip_qpc_ticks: i64,
    pub user_cpu_100ns: Metric<u64>,
    pub kernel_cpu_100ns: Metric<u64>,
    pub private_working_set_bytes: Metric<u64>,
    pub private_commit_bytes: Metric<u64>,
    pub total_working_set_bytes: Metric<u64>,
    pub derived_cpu: CpuDerivedRecord,
    pub sampler_delta_user_cpu_100ns: Metric<u64>,
    pub sampler_delta_kernel_cpu_100ns: Metric<u64>,
    pub helper_snapshot: HelperSnapshotRecord,
    pub terminal_sample: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BinaryIdentity {
    pub canonical_path: String,
    pub sha256: Metric<String>,
    pub size_bytes: Metric<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentRecord {
    pub os_version: Metric<String>,
    pub architecture: Metric<String>,
    pub architecture_provenance: String,
    pub logical_processor_count: Metric<u32>,
    pub installed_ram_bytes: Metric<u64>,
    pub cpu_identity: Metric<String>,
    pub cpu_identity_provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Counts {
    pub samples: u64,
    pub valid_cpu_observations: u64,
    pub invalid_cpu_observations: u64,
    pub valid_private_working_set_observations: u64,
    pub valid_private_commit_observations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema: String,
    pub tool_version: String,
    pub run_id: String,
    pub state: String,
    pub started_utc_filetime: u64,
    pub ended_utc_filetime: Option<u64>,
    pub sampler: BinaryIdentity,
    pub target: BinaryIdentity,
    pub target_arguments: Vec<String>,
    pub target_working_directory: String,
    pub sampler_pid: u32,
    pub target_pid: Option<u32>,
    pub target_creation_filetime: Option<u64>,
    pub target_exit_filetime: Option<u64>,
    pub interval_ms: u64,
    pub maximum_duration_ms: u64,
    pub qpc_frequency_hz: u64,
    pub clocks: BTreeMap<String, String>,
    pub sampling_scope: String,
    pub metric_provenance: BTreeMap<String, String>,
    pub ex2_support: String,
    pub logical_processor_normalization: String,
    pub environment: EnvironmentRecord,
    pub declarations: BTreeMap<String, String>,
    pub optional_run_context: BTreeMap<String, String>,
    pub collection_stop_reason: Option<String>,
    pub collection_valid: bool,
    pub tool_exit_success: bool,
    pub required_live_query_failures: u64,
    pub target_exit_code: Option<u32>,
    pub harness_error: Option<String>,
    pub cleanup_error: Option<String>,
    pub counts: Counts,
    pub observed_helper_pids: Vec<u32>,
    pub observed_helper_pids_truncated: bool,
    pub helper_snapshot_failures: u64,
    pub helper_snapshot_truncations: u64,
    pub helper_visibility_limit: String,
    pub sampler_cpu_delta_user_100ns: Option<u64>,
    pub sampler_cpu_delta_kernel_100ns: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_is_distinct_from_a_real_zero_and_round_trips() {
        let zero = Metric::Valid { value: 0_u64 };
        let unavailable: Metric<u64> = Metric::Unavailable {
            error: ApiError {
                api: "GetProcessTimes".into(),
                os_code: 5,
                reason: "access_denied".into(),
            },
        };
        let z = serde_json::to_string(&zero).unwrap();
        let u = serde_json::to_string(&unavailable).unwrap();
        assert!(z.contains("\"value\":0"));
        assert!(!u.contains("\"value\""));
        assert_ne!(z, u);
        assert_eq!(
            serde_json::from_str::<Metric<u64>>(&u).unwrap(),
            unavailable
        );
    }

    #[test]
    fn sample_is_one_parseable_json_line_without_nonfinite_values() {
        let sample = SampleRecord {
            schema: SCHEMA_VERSION.into(),
            run_id: "run-1".into(),
            sequence: 0,
            scheduled_qpc: 1,
            sample_start_qpc: 2,
            cpu_observation_qpc: 3,
            sample_end_qpc: 4,
            schedule_slip_qpc_ticks: 1,
            user_cpu_100ns: Metric::Valid { value: 0 },
            kernel_cpu_100ns: Metric::Valid { value: 0 },
            private_working_set_bytes: Metric::Unsupported {
                reason: "host_too_old".into(),
            },
            private_commit_bytes: Metric::Valid { value: 10 },
            total_working_set_bytes: Metric::Valid { value: 20 },
            derived_cpu: CpuDerivedRecord {
                baseline_sequence: None,
                spans_invalid_observation: false,
                delta_user_100ns: None,
                delta_kernel_100ns: None,
                elapsed_qpc_ticks: None,
                elapsed_100ns: None,
                cpu_percent_one_logical: None,
                cpu_percent_machine: None,
                unavailable_reasons: vec!["first_observation".into()],
            },
            sampler_delta_user_cpu_100ns: Metric::Valid { value: 0 },
            sampler_delta_kernel_cpu_100ns: Metric::Valid { value: 0 },
            helper_snapshot: HelperSnapshotRecord::Complete {
                pids: vec![42],
                assigned: 1,
            },
            terminal_sample: false,
        };
        let json = serde_json::to_string(&sample).unwrap();
        assert!(!json.contains('\n'));
        assert_eq!(serde_json::from_str::<SampleRecord>(&json).unwrap(), sample);
    }
}
