-- Requirement Profiles are published evidence. They no longer create a
-- user-confirmation request or participate in the analysis readiness gate.
-- InputRequest rows are a rebuildable projection; remove every obsolete
-- profile-confirmation projection before tightening the current kind contract,
-- including answered and superseded historical rows.
DELETE FROM data_intake_input_requests
WHERE kind = 'confirm_profile';

ALTER TABLE data_intake_input_requests
    DROP CONSTRAINT data_intake_input_requests_kind_check;

ALTER TABLE data_intake_input_requests
    ADD CONSTRAINT data_intake_input_requests_kind_check CHECK (kind IN (
        'confirm_mapping', 'answer_parameters', 'provide_data', 'confirm_analysis'
    ));

ALTER TABLE data_intake_sessions
    DROP COLUMN profile_confirmation_sha256;
