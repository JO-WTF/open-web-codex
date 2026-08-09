ALTER TABLE approvals
    DROP CONSTRAINT approvals_decision_check;

ALTER TABLE approvals
    ADD CONSTRAINT approvals_decision_check
    CHECK (
        decision IS NULL
        OR decision IN ('approved', 'rejected', 'answered', 'accept', 'decline', 'cancel')
    );
