use ceh_desktop::update_job::{Phase, UpdateJob};
use ceh_services::updates::UpdateError;
use std::{path::PathBuf, sync::atomic::Ordering, time::Duration};

fn wait_for(job: &UpdateJob, wanted: Phase) -> ceh_desktop::update_job::Status {
    for _ in 0..200 {
        let status = job.status();
        if status.phase == wanted {
            return status;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("never reached {wanted:?}: {:?}", job.status());
}

#[test]
fn a_download_reports_progress_and_ends_with_the_package_ready_to_install() {
    let job = UpdateJob::new();
    assert_eq!(job.status().phase, Phase::Idle);
    assert!(job.install_path().is_none());
    assert!(job.start_with(|_cancel, progress| {
        progress(40);
        Ok(PathBuf::from("/tmp/package.dmg"))
    }));
    let done = wait_for(&job, Phase::Ready);
    assert_eq!(done.percent, 100);
    assert_eq!(job.install_path(), Some(PathBuf::from("/tmp/package.dmg")));
}

#[test]
fn only_one_download_runs_and_a_failure_names_why() {
    let job = UpdateJob::new();
    assert!(job.start_with(|cancel, _| {
        while !cancel.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(UpdateError::Cancelled)
    }));
    assert!(!job.start_with(|_, _| Ok(PathBuf::new())), "a second download is refused while one runs");
    job.cancel();
    assert_eq!(wait_for(&job, Phase::Failed).error, Some("cancelled"));
    assert!(job.install_path().is_none(), "nothing to install after a failure");
    assert!(job.start_with(|_, _| Err(UpdateError::Invalid)));
    assert_eq!(wait_for(&job, Phase::Failed).error, Some("verification_failed"));
    assert!(job.start_with(|_, _| Err(UpdateError::Network)));
    assert_eq!(wait_for(&job, Phase::Failed).error, Some("network"));
}
