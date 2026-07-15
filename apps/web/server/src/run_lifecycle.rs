/// Run status transitions for the platform orchestrator.
pub const RUN_STATUSES: &[&str] = &[
    "queued",
    "provisioning",
    "pending",
    "running",
    "waiting_approval",
    "completed",
    "cancelled",
    "failed",
];

const TERMINAL: &[&str] = &["completed", "cancelled", "failed"];

pub fn can_transition(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    if TERMINAL.contains(&from) {
        return false;
    }
    matches!(
        (from, to),
        ("queued", "provisioning")
            | ("queued", "failed")
            | ("queued", "cancelled")
            | ("provisioning", "running")
            | ("provisioning", "failed")
            | ("provisioning", "cancelled")
            | ("pending", "running")
            | ("pending", "failed")
            | ("pending", "cancelled")
            | ("running", "waiting_approval")
            | ("running", "completed")
            | ("running", "failed")
            | ("running", "cancelled")
            | ("waiting_approval", "running")
            | ("waiting_approval", "failed")
            | ("waiting_approval", "cancelled")
    )
}

pub fn assert_transition(from: &str, to: &str) -> Result<(), String> {
    if can_transition(from, to) {
        Ok(())
    } else {
        Err(format!("invalid run transition {from} -> {to}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_run_can_enter_provisioning() {
        assert!(can_transition("queued", "provisioning"));
    }

    #[test]
    fn completed_run_cannot_restart() {
        assert!(!can_transition("completed", "running"));
    }
}
