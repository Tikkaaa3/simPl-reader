//! Generation independence from the current working directory.
//!
//! Isolated in its own test binary because changing the process working
//! directory is global state.

use reader_workload::{WorkloadSize, workload};

#[test]
fn generation_does_not_depend_on_the_current_directory() {
    let baseline = workload(WorkloadSize::Small);
    let large_baseline = workload(WorkloadSize::Large);

    let previous = std::env::current_dir().expect("current directory readable");
    std::env::set_current_dir(std::env::temp_dir()).expect("change to temp directory");
    let from_temp = workload(WorkloadSize::Small);
    let large_from_temp = workload(WorkloadSize::Large);
    std::env::set_current_dir(previous).expect("restore directory");

    assert_eq!(baseline, from_temp);
    assert_eq!(large_baseline, large_from_temp);
}
