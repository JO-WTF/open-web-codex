use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use open_web_codex_platform_contracts::{
    ProviderCatalog, ProviderCredentialInput, ProviderModelSummary, UpdateProviderModelRequest,
    UpsertProviderRequest,
};
use open_web_codex_profile_registry::{ProfileRegistry, ProfileRegistryError};
use open_web_codex_secret_store::{PostgresSecretStore, SecretStoreError, SecretValue};
use serde_json::Value;
use sqlx::{PgPool, Row};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    InMemoryProviderService, ProviderOperations, ProviderService, ProviderServiceError,
    ProviderTransport,
};

#[derive(Debug, Error)]
pub enum AuthorizedProviderError {
    #[error("Provider access is not authorized")]
    Forbidden,
    #[error(transparent)]
    Provider(#[from] ProviderServiceError),
    #[error("Profile Registry operation failed: {0}")]
    Registry(#[from] ProfileRegistryError),
    #[error("Provider Secret operation failed: {0}")]
    Secret(#[from] SecretStoreError),
    #[error("Provider authorization database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait AuthorizedProviderOperations: Send + Sync {
    async fn list(&self, actor: ProviderActor) -> Result<ProviderCatalog, AuthorizedProviderError>;
    async fn upsert(
        &self,
        actor: ProviderActor,
        id: &str,
        request: UpsertProviderRequest,
    ) -> Result<ProviderCatalog, AuthorizedProviderError>;
    async fn select(
        &self,
        actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError>;
    async fn select_model(
        &self,
        actor: ProviderActor,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError>;
    async fn delete(
        &self,
        actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError>;
    async fn refresh_models(
        &self,
        actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError>;
    async fn update_model(
        &self,
        actor: ProviderActor,
        provider_id: &str,
        model_id: &str,
        request: UpdateProviderModelRequest,
    ) -> Result<ProviderCatalog, AuthorizedProviderError>;
}

#[derive(Clone, Copy, Debug)]
pub struct ProviderActor {
    pub user_id: Uuid,
    pub organization_id: Uuid,
}

struct RegistryProviderTransport {
    registry: ProfileRegistry,
    runtime_key: String,
}

#[async_trait]
impl ProviderTransport for RegistryProviderTransport {
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let host = self
            .registry
            .host(&self.runtime_key)
            .await
            .map_err(|error| error.to_string())?;
        host.request(method, params)
            .await
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub struct SecuredProviderService {
    db: PgPool,
    runtime_key: String,
    registry: ProfileRegistry,
    secrets: PostgresSecretStore,
    runtime: ProviderService,
    operation: Arc<Mutex<()>>,
}

#[derive(Clone, Copy)]
struct AuthorizedProfile {
    id: Uuid,
    organization_id: Uuid,
}

enum PersistedEnvironmentKey {
    Preserve,
    Replace(Option<String>),
}

impl SecuredProviderService {
    pub fn new(
        db: PgPool,
        runtime_key: impl Into<String>,
        registry: ProfileRegistry,
        secrets: PostgresSecretStore,
    ) -> Self {
        let runtime_key = runtime_key.into();
        let runtime = ProviderService::new(Arc::new(RegistryProviderTransport {
            registry: registry.clone(),
            runtime_key: runtime_key.clone(),
        }));
        Self {
            db,
            runtime_key,
            registry,
            secrets,
            runtime,
            operation: Arc::new(Mutex::new(())),
        }
    }

    /// Load persisted Provider Secrets before initially spawning the Profile.
    /// Values are returned only to the Profile Registry and must never be logged.
    pub async fn startup_secret_environment(
        &self,
    ) -> Result<BTreeMap<String, String>, AuthorizedProviderError> {
        let Some(profile) = self.profile_identity().await? else {
            return Ok(BTreeMap::new());
        };
        let values = self
            .secrets
            .list_provider_environment(profile.organization_id, profile.id)
            .await?;
        Ok(values
            .into_iter()
            .map(|value| (value.environment_key, value.value.expose().to_string()))
            .collect())
    }

    /// Rebuild Runtime provider configuration from the durable platform
    /// definition after the Profile Host has initialized.
    pub async fn restore_persisted_configuration(&self) -> Result<(), AuthorizedProviderError> {
        let Some(profile) = self.profile_identity().await? else {
            return Ok(());
        };
        let rows = sqlx::query(
            "SELECT provider_id, name, base_url, wire_api, supports_function_tools, models, is_selected, selected_model_id, credential_env_key \
             FROM profile_provider_definitions WHERE profile_id = $1 ORDER BY provider_id",
        )
        .bind(profile.id)
        .fetch_all(&self.db)
        .await?;
        let environments = self
            .secrets
            .list_provider_environment(profile.organization_id, profile.id)
            .await?
            .into_iter()
            .map(|entry| (entry.provider_id, entry.environment_key))
            .collect::<BTreeMap<_, _>>();
        let mut selected_provider = None;
        let mut selected_model: Option<String> = None;
        for row in rows {
            let provider_id: String = row.get("provider_id");
            let persisted_environment_key: Option<String> = row.get("credential_env_key");
            self.runtime
                .upsert(
                    &provider_id,
                    UpsertProviderRequest {
                        name: row.get("name"),
                        base_url: row.get("base_url"),
                        wire_api: row.get("wire_api"),
                        credentials: environments
                            .get(&provider_id)
                            .cloned()
                            .or(persisted_environment_key)
                            .map(|env_key| ProviderCredentialInput::Environment { env_key })
                            .unwrap_or(ProviderCredentialInput::NoCredential),
                        supports_function_tools: Some(row.get("supports_function_tools")),
                        select: false,
                    },
                )
                .await?;
            let models: Vec<ProviderModelSummary> = serde_json::from_value(row.get("models"))
                .map_err(|error| ProviderServiceError::InvalidResponse(error.to_string()))?;
            for model in models {
                self.runtime
                    .update_model(
                        &provider_id,
                        &model.model_id,
                        UpdateProviderModelRequest {
                            context_window: model.context_window.unwrap_or(128_000),
                            supports_search_tool: Some(model.supports_search_tool),
                        },
                    )
                    .await?;
            }
            if row.get::<bool, _>("is_selected") {
                selected_provider = Some(provider_id);
                selected_model = row.get::<Option<String>, _>("selected_model_id");
            }
        }
        if let Some(provider_id) = selected_provider {
            self.runtime.select(&provider_id).await?;
            if let Some(model_id) = selected_model {
                self.runtime.select_model(&provider_id, &model_id).await?;
            }
        }
        self.runtime.ensure_default_reasoning_effort().await?;
        Ok(())
    }

    async fn persist_catalog_provider(
        &self,
        profile_id: Uuid,
        catalog: &ProviderCatalog,
        provider_id: &str,
        selected_model_id: Option<&str>,
        environment_key: PersistedEnvironmentKey,
    ) -> Result<(), AuthorizedProviderError> {
        let provider = catalog
            .data
            .iter()
            .find(|provider| provider.id == provider_id)
            .ok_or_else(|| ProviderServiceError::NotFound(provider_id.to_string()))?;
        let models = serde_json::to_value(&provider.models)
            .map_err(|error| ProviderServiceError::InvalidResponse(error.to_string()))?;
        let (credential_env_key, preserve_credential_env_key) = match environment_key {
            PersistedEnvironmentKey::Preserve => (None, true),
            PersistedEnvironmentKey::Replace(value) => (value, false),
        };
        sqlx::query(
            "INSERT INTO profile_provider_definitions \
             (profile_id, provider_id, name, base_url, wire_api, supports_function_tools, models, is_selected, selected_model_id, credential_env_key) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
             ON CONFLICT (profile_id, provider_id) DO UPDATE SET \
               name = EXCLUDED.name, base_url = EXCLUDED.base_url, wire_api = EXCLUDED.wire_api, \
               supports_function_tools = EXCLUDED.supports_function_tools, models = EXCLUDED.models, \
               is_selected = EXCLUDED.is_selected, \
               selected_model_id = COALESCE(EXCLUDED.selected_model_id, profile_provider_definitions.selected_model_id), \
               credential_env_key = CASE WHEN $11 THEN profile_provider_definitions.credential_env_key ELSE EXCLUDED.credential_env_key END, \
               updated_at = now()",
        )
        .bind(profile_id)
        .bind(&provider.id)
        .bind(&provider.name)
        .bind(provider.base_url.as_deref().unwrap_or_default())
        .bind(&provider.wire_api)
        .bind(provider.supports_function_tools)
        .bind(models)
        .bind(provider.is_current)
        .bind(selected_model_id)
        .bind(credential_env_key)
        .bind(preserve_credential_env_key)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn overlay_persisted_models(
        &self,
        profile_id: Uuid,
        mut catalog: ProviderCatalog,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        let rows = sqlx::query(
            "SELECT provider_id, models FROM profile_provider_definitions \
             WHERE profile_id = $1 ORDER BY provider_id",
        )
        .bind(profile_id)
        .fetch_all(&self.db)
        .await?;
        for row in rows {
            let provider_id: String = row.get("provider_id");
            let Some(provider) = catalog
                .data
                .iter_mut()
                .find(|provider| provider.id == provider_id)
            else {
                continue;
            };
            let models: Vec<ProviderModelSummary> = serde_json::from_value(row.get("models"))
                .map_err(|error| ProviderServiceError::InvalidResponse(error.to_string()))?;
            provider.model_count = models.len();
            provider.models = models;
        }
        Ok(catalog)
    }

    async fn persisted_models(
        &self,
        profile_id: Uuid,
        provider_id: &str,
    ) -> Result<Vec<ProviderModelSummary>, AuthorizedProviderError> {
        let row = sqlx::query(
            "SELECT models FROM profile_provider_definitions \
             WHERE profile_id = $1 AND provider_id = $2",
        )
        .bind(profile_id)
        .bind(provider_id)
        .fetch_optional(&self.db)
        .await?;
        row.map(|row| {
            serde_json::from_value(row.get("models"))
                .map_err(|error| ProviderServiceError::InvalidResponse(error.to_string()).into())
        })
        .transpose()
        .map(|models| models.unwrap_or_default())
    }

    fn merge_persisted_model_capabilities(
        catalog: &mut ProviderCatalog,
        provider_id: &str,
        persisted_models: &[ProviderModelSummary],
    ) {
        let Some(provider) = catalog
            .data
            .iter_mut()
            .find(|provider| provider.id == provider_id)
        else {
            return;
        };
        for model in &mut provider.models {
            if let Some(persisted) = persisted_models
                .iter()
                .find(|candidate| candidate.model_id == model.model_id)
            {
                model.supports_search_tool = persisted.supports_search_tool;
            }
        }
    }

    async fn authorize(
        &self,
        actor: ProviderActor,
    ) -> Result<AuthorizedProfile, AuthorizedProviderError> {
        let row = sqlx::query(
            "SELECT p.id, p.organization_id FROM profiles p \
             JOIN memberships m ON m.organization_id = p.organization_id AND m.user_id = $1 \
             WHERE p.owner_user_id = $1 AND p.organization_id = $2 \
               AND p.runtime_key = $3 AND p.status = 'active'",
        )
        .bind(actor.user_id)
        .bind(actor.organization_id)
        .bind(&self.runtime_key)
        .fetch_optional(&self.db)
        .await?
        .ok_or(AuthorizedProviderError::Forbidden)?;
        Ok(AuthorizedProfile {
            id: row.get("id"),
            organization_id: row.get("organization_id"),
        })
    }

    async fn profile_identity(&self) -> Result<Option<AuthorizedProfile>, AuthorizedProviderError> {
        let row = sqlx::query(
            "SELECT id, organization_id FROM profiles WHERE runtime_key = $1 AND status = 'active'",
        )
        .bind(&self.runtime_key)
        .fetch_optional(&self.db)
        .await?;
        Ok(row.map(|row| AuthorizedProfile {
            id: row.get("id"),
            organization_id: row.get("organization_id"),
        }))
    }

    async fn restore_secret(
        &self,
        profile: AuthorizedProfile,
        provider_id: &str,
        previous: Option<SecretValue>,
    ) -> Result<(), AuthorizedProviderError> {
        match previous {
            Some(previous) => {
                let environment_key = self
                    .secrets
                    .put_provider_key(profile.organization_id, profile.id, provider_id, &previous)
                    .await?;
                self.registry
                    .set_secret_environment(
                        &self.runtime_key,
                        &environment_key,
                        Some(previous.expose().to_string()),
                    )
                    .await?;
            }
            None => {
                let environment_key = self
                    .secrets
                    .delete_provider_key(profile.organization_id, profile.id, provider_id)
                    .await?;
                self.registry
                    .set_secret_environment(&self.runtime_key, &environment_key, None)
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl AuthorizedProviderOperations for SecuredProviderService {
    async fn list(&self, actor: ProviderActor) -> Result<ProviderCatalog, AuthorizedProviderError> {
        let profile = self.authorize(actor).await?;
        let catalog = self.runtime.list().await?;
        self.overlay_persisted_models(profile.id, catalog).await
    }

    async fn upsert(
        &self,
        actor: ProviderActor,
        id: &str,
        mut request: UpsertProviderRequest,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        let _operation = self.operation.lock().await;
        let profile = self.authorize(actor).await?;
        let credentials =
            std::mem::replace(&mut request.credentials, ProviderCredentialInput::Preserve);
        match credentials {
            ProviderCredentialInput::Direct { api_key } => {
                let previous = self
                    .secrets
                    .get_provider_key(profile.organization_id, profile.id, id)
                    .await?;
                let value = SecretValue::new(api_key)?;
                let environment_key = self
                    .secrets
                    .put_provider_key(profile.organization_id, profile.id, id, &value)
                    .await?;
                if let Err(error) = self
                    .registry
                    .set_secret_environment(
                        &self.runtime_key,
                        &environment_key,
                        Some(value.expose().to_string()),
                    )
                    .await
                {
                    self.restore_secret(profile, id, previous).await?;
                    return Err(error.into());
                }
                request.credentials = ProviderCredentialInput::Environment {
                    env_key: environment_key,
                };
                match self.runtime.upsert(id, request).await {
                    Ok(catalog) => {
                        self.persist_catalog_provider(
                            profile.id,
                            &catalog,
                            id,
                            None,
                            PersistedEnvironmentKey::Replace(None),
                        )
                        .await?;
                        Ok(catalog)
                    }
                    Err(error) => {
                        self.restore_secret(profile, id, previous).await?;
                        Err(error.into())
                    }
                }
            }
            credentials @ (ProviderCredentialInput::Environment { .. }
            | ProviderCredentialInput::NoCredential) => {
                let persisted_environment_key = match &credentials {
                    ProviderCredentialInput::Environment { env_key } => {
                        PersistedEnvironmentKey::Replace(Some(env_key.trim().to_string()))
                    }
                    ProviderCredentialInput::NoCredential => PersistedEnvironmentKey::Replace(None),
                    _ => unreachable!("matched environment or no-credential input"),
                };
                request.credentials = credentials;
                let previous = self
                    .secrets
                    .get_provider_key(profile.organization_id, profile.id, id)
                    .await?;
                if previous.is_some() {
                    let environment_key = self
                        .secrets
                        .delete_provider_key(profile.organization_id, profile.id, id)
                        .await?;
                    if let Err(error) = self
                        .registry
                        .set_secret_environment(&self.runtime_key, &environment_key, None)
                        .await
                    {
                        self.restore_secret(profile, id, previous).await?;
                        return Err(error.into());
                    }
                }
                match self.runtime.upsert(id, request).await {
                    Ok(catalog) => {
                        self.persist_catalog_provider(
                            profile.id,
                            &catalog,
                            id,
                            None,
                            persisted_environment_key,
                        )
                        .await?;
                        Ok(catalog)
                    }
                    Err(error) => {
                        self.restore_secret(profile, id, previous).await?;
                        Err(error.into())
                    }
                }
            }
            ProviderCredentialInput::Preserve => {
                let catalog = self.runtime.upsert(id, request).await?;
                self.persist_catalog_provider(
                    profile.id,
                    &catalog,
                    id,
                    None,
                    PersistedEnvironmentKey::Preserve,
                )
                .await?;
                Ok(catalog)
            }
        }
    }

    async fn select(
        &self,
        actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        let profile = self.authorize(actor).await?;
        let catalog = self.runtime.select(id).await?;
        sqlx::query(
            "UPDATE profile_provider_definitions SET is_selected = (provider_id = $2), updated_at = now() WHERE profile_id = $1",
        )
        .bind(profile.id)
        .bind(id)
        .execute(&self.db)
        .await?;
        self.overlay_persisted_models(profile.id, catalog).await
    }

    async fn select_model(
        &self,
        actor: ProviderActor,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        let profile = self.authorize(actor).await?;
        let catalog = self.runtime.select_model(provider_id, model_id).await?;
        sqlx::query(
            "UPDATE profile_provider_definitions SET is_selected = (provider_id = $2), selected_model_id = CASE WHEN provider_id = $2 THEN $3 ELSE selected_model_id END, updated_at = now() WHERE profile_id = $1",
        )
        .bind(profile.id)
        .bind(provider_id)
        .bind(model_id)
        .execute(&self.db)
        .await?;
        self.overlay_persisted_models(profile.id, catalog).await
    }

    async fn delete(
        &self,
        actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        let _operation = self.operation.lock().await;
        let profile = self.authorize(actor).await?;
        let previous = self
            .secrets
            .get_provider_key(profile.organization_id, profile.id, id)
            .await?;
        if previous.is_some() {
            let environment_key = self
                .secrets
                .delete_provider_key(profile.organization_id, profile.id, id)
                .await?;
            if let Err(error) = self
                .registry
                .set_secret_environment(&self.runtime_key, &environment_key, None)
                .await
            {
                self.restore_secret(profile, id, previous).await?;
                return Err(error.into());
            }
        }
        match self.runtime.delete(id).await {
            Ok(catalog) => {
                sqlx::query(
                    "DELETE FROM profile_provider_definitions WHERE profile_id = $1 AND provider_id = $2",
                )
                .bind(profile.id)
                .bind(id)
                .execute(&self.db)
                .await?;
                self.overlay_persisted_models(profile.id, catalog).await
            }
            Err(error) => {
                self.restore_secret(profile, id, previous).await?;
                Err(error.into())
            }
        }
    }

    async fn refresh_models(
        &self,
        actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        let _operation = self.operation.lock().await;
        let profile = self.authorize(actor).await?;
        let persisted_models = self.persisted_models(profile.id, id).await?;
        let mut catalog = self.runtime.refresh_models(id).await?;
        Self::merge_persisted_model_capabilities(&mut catalog, id, &persisted_models);
        self.persist_catalog_provider(
            profile.id,
            &catalog,
            id,
            None,
            PersistedEnvironmentKey::Preserve,
        )
        .await?;
        self.registry
            .schedule_runtime_refresh(&self.runtime_key)
            .await?;
        Ok(catalog)
    }

    async fn update_model(
        &self,
        actor: ProviderActor,
        provider_id: &str,
        model_id: &str,
        request: UpdateProviderModelRequest,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        let _operation = self.operation.lock().await;
        let profile = self.authorize(actor).await?;
        let persisted_models = self.persisted_models(profile.id, provider_id).await?;
        let context_window = request.context_window;
        let supports_search_tool = request.supports_search_tool;
        let mut catalog = self
            .runtime
            .update_model(provider_id, model_id, request)
            .await?;
        let has_runtime_models = catalog
            .data
            .iter()
            .find(|provider| provider.id == provider_id)
            .is_some_and(|provider| !provider.models.is_empty());
        if has_runtime_models {
            Self::merge_persisted_model_capabilities(&mut catalog, provider_id, &persisted_models);
        }
        if let Some(provider) = catalog
            .data
            .iter_mut()
            .find(|provider| provider.id == provider_id)
        {
            if provider.models.is_empty() && !persisted_models.is_empty() {
                provider.models = persisted_models.clone();
            }
            if let Some(model) = provider
                .models
                .iter_mut()
                .find(|model| model.model_id == model_id)
            {
                model.context_window = Some(context_window);
                if let Some(supports_search_tool) = supports_search_tool {
                    model.supports_search_tool = supports_search_tool;
                }
            } else {
                provider.models.push(ProviderModelSummary {
                    model_id: model_id.to_string(),
                    model_name: Some(model_id.to_string()),
                    max_token_len: None,
                    max_output_tokens: None,
                    show_in_picker: true,
                    context_window: Some(context_window),
                    supports_search_tool: supports_search_tool.unwrap_or(false),
                });
            }
            provider.model_count = provider.models.len();
        }
        self.persist_catalog_provider(
            profile.id,
            &catalog,
            provider_id,
            None,
            PersistedEnvironmentKey::Preserve,
        )
        .await?;
        self.registry
            .schedule_runtime_refresh(&self.runtime_key)
            .await?;
        Ok(catalog)
    }
}

#[derive(Clone, Default)]
pub struct InMemoryAuthorizedProviderService {
    inner: InMemoryProviderService,
}

#[async_trait]
impl AuthorizedProviderOperations for InMemoryAuthorizedProviderService {
    async fn list(
        &self,
        _actor: ProviderActor,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        Ok(self.inner.list().await?)
    }

    async fn upsert(
        &self,
        _actor: ProviderActor,
        id: &str,
        request: UpsertProviderRequest,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        Ok(self.inner.upsert(id, request).await?)
    }

    async fn select(
        &self,
        _actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        Ok(self.inner.select(id).await?)
    }

    async fn select_model(
        &self,
        _actor: ProviderActor,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        Ok(self.inner.select_model(provider_id, model_id).await?)
    }

    async fn delete(
        &self,
        _actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        Ok(self.inner.delete(id).await?)
    }

    async fn refresh_models(
        &self,
        _actor: ProviderActor,
        id: &str,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        Ok(self.inner.refresh_models(id).await?)
    }

    async fn update_model(
        &self,
        _actor: ProviderActor,
        provider_id: &str,
        model_id: &str,
        request: UpdateProviderModelRequest,
    ) -> Result<ProviderCatalog, AuthorizedProviderError> {
        Ok(self
            .inner
            .update_model(provider_id, model_id, request)
            .await?)
    }
}
