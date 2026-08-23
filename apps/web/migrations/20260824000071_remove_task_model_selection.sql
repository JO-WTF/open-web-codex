DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM tasks
        WHERE model_provider IS NOT NULL OR model IS NOT NULL
        LIMIT 1
    ) THEN
        RAISE EXCEPTION
            'tasks contains legacy Provider/model state; run ./scripts/rebuild-development-database.sh --confirm-development-only before applying this migration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM platform_configuration
        WHERE scope_kind = 'global'
          AND scope_id = 'global'
          AND config_key = 'models.default_selection'
        LIMIT 1
    ) THEN
        RAISE EXCEPTION
            'platform_configuration contains legacy models.default_selection; run ./scripts/rebuild-development-database.sh --confirm-development-only before applying this migration';
    END IF;
END $$;

ALTER TABLE tasks DROP CONSTRAINT tasks_model_selection_pair_check;

ALTER TABLE tasks
    DROP COLUMN model_provider,
    DROP COLUMN model;
