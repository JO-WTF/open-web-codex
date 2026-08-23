DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM profile_provider_definitions LIMIT 1) THEN
        RAISE EXCEPTION
            'profile_provider_definitions contains legacy Provider state; run ./scripts/rebuild-development-database.sh --confirm-development-only before applying this migration';
    END IF;
END $$;

DROP TABLE profile_provider_definitions;
