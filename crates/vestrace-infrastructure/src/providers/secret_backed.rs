//! A provider factory that resolves its credential from the secret store.
//!
//! The alternative — reading a key from the environment at start-up and holding
//! it for the process's lifetime — would mean one key for every workspace the
//! process serves, no rotation without a restart, and a plaintext credential
//! resident in memory the whole time. All three are avoided by resolving per
//! invocation.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, RequestContext, ResolvedTextGenerationProvider, SharedSecretStore,
    TextGenerationProviderFactory,
};
use vestrace_domain::trust::SecretResolutionRequest;

use super::OpenAiCompatibleClient;

/// The purpose a provider credential is filed under.
///
/// A resolution lease must name the purpose, and the domain refuses a lease
/// whose purpose does not match — so a caller authorized for, say, export
/// signing cannot be handed a provider key.
pub const PROVIDER_API_KEY_PURPOSE: &str = "provider-api-key";

pub struct SecretBackedProviderFactory {
    base_url: String,
    secrets: SharedSecretStore,
    /// Name of the secret holding this provider's key.
    secret_name: String,
}

impl std::fmt::Debug for SecretBackedProviderFactory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretBackedProviderFactory")
            .field("base_url", &self.base_url)
            .field("secret_name", &self.secret_name)
            .finish()
    }
}

impl SecretBackedProviderFactory {
    pub fn new(
        base_url: impl Into<String>,
        secrets: SharedSecretStore,
        secret_name: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            secrets,
            secret_name: secret_name.into(),
        }
    }
}

#[async_trait]
impl TextGenerationProviderFactory for SecretBackedProviderFactory {
    async fn provider_for(
        &self,
        context: &RequestContext,
    ) -> Result<ResolvedTextGenerationProvider, ApplicationError> {
        let reference = self
            .secrets
            .find(context, &self.secret_name, PROVIDER_API_KEY_PURPOSE)
            .await?
            .ok_or_else(|| {
                // Names what is missing, not what the key would have been.
                ApplicationError::InvalidConfiguration(format!(
                    "no secret named {:?} with purpose {PROVIDER_API_KEY_PURPOSE:?} exists in this workspace",
                    self.secret_name
                ))
            })?;

        // The lease is what carries authorization: it is refused unless the
        // workspace and purpose match the stored reference.
        let lease = reference
            .authorize_resolution(&SecretResolutionRequest::new(
                context.workspace_id,
                PROVIDER_API_KEY_PURPOSE,
                format!("run-step-execution:{}", context.principal_id.as_uuid()),
            ))
            .map_err(ApplicationError::Domain)?;

        let material = self.secrets.resolve(context, &lease).await?;
        let api_key = material.expose_str()?.to_string();

        let client = OpenAiCompatibleClient::new(&self.base_url, Some(api_key))
            .map_err(|error| ApplicationError::Unavailable(error.to_string()))?;
        let egress = client.egress().clone();
        Ok(ResolvedTextGenerationProvider {
            provider: Arc::new(client),
            egress,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    struct OneSecret {
        reference: vestrace_domain::trust::SecretRef,
    }

    #[async_trait]
    impl vestrace_application::SecretStore for OneSecret {
        async fn put(
            &self,
            _: &RequestContext,
            _: &str,
            _: &str,
            _: vestrace_application::SecretMaterial,
        ) -> Result<vestrace_domain::trust::SecretRef, ApplicationError> {
            unreachable!()
        }

        async fn list(
            &self,
            _: &RequestContext,
        ) -> Result<Vec<vestrace_application::SecretDescriptor>, ApplicationError> {
            Ok(vec![])
        }

        async fn find(
            &self,
            _: &RequestContext,
            _: &str,
            _: &str,
        ) -> Result<Option<vestrace_domain::trust::SecretRef>, ApplicationError> {
            Ok(Some(self.reference.clone()))
        }

        async fn resolve(
            &self,
            _: &RequestContext,
            _: &vestrace_domain::trust::SecretLease,
        ) -> Result<vestrace_application::SecretMaterial, ApplicationError> {
            Ok(vestrace_application::SecretMaterial::new(
                b"local-development-key".to_vec(),
            ))
        }

        async fn delete(
            &self,
            _: &RequestContext,
            _: vestrace_domain::SecretRefId,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[test]
    fn the_factory_never_renders_a_credential() {
        // It holds only the secret's *name*, but the Debug impl is explicit so
        // that adding a cached key later cannot start printing it.
        struct NoSecrets;
        #[async_trait]
        impl vestrace_application::SecretStore for NoSecrets {
            async fn put(
                &self,
                _: &RequestContext,
                _: &str,
                _: &str,
                _: vestrace_application::SecretMaterial,
            ) -> Result<vestrace_domain::trust::SecretRef, ApplicationError> {
                unreachable!()
            }
            async fn list(
                &self,
                _: &RequestContext,
            ) -> Result<Vec<vestrace_application::SecretDescriptor>, ApplicationError> {
                Ok(vec![])
            }
            async fn find(
                &self,
                _: &RequestContext,
                _: &str,
                _: &str,
            ) -> Result<Option<vestrace_domain::trust::SecretRef>, ApplicationError> {
                Ok(None)
            }
            async fn resolve(
                &self,
                _: &RequestContext,
                _: &vestrace_domain::trust::SecretLease,
            ) -> Result<vestrace_application::SecretMaterial, ApplicationError> {
                unreachable!()
            }
            async fn delete(
                &self,
                _: &RequestContext,
                _: vestrace_domain::SecretRefId,
            ) -> Result<(), ApplicationError> {
                Ok(())
            }
        }

        let factory = SecretBackedProviderFactory::new(
            "https://api.example.com/v1",
            Arc::new(NoSecrets),
            "openai",
        );
        let rendered = format!("{factory:?}");
        assert!(rendered.contains("openai"));
        assert!(rendered.contains("api.example.com"));
    }

    #[tokio::test]
    async fn an_absent_credential_is_a_configuration_error_naming_only_the_secret() {
        struct NoSecrets;
        #[async_trait]
        impl vestrace_application::SecretStore for NoSecrets {
            async fn put(
                &self,
                _: &RequestContext,
                _: &str,
                _: &str,
                _: vestrace_application::SecretMaterial,
            ) -> Result<vestrace_domain::trust::SecretRef, ApplicationError> {
                unreachable!()
            }
            async fn list(
                &self,
                _: &RequestContext,
            ) -> Result<Vec<vestrace_application::SecretDescriptor>, ApplicationError> {
                Ok(vec![])
            }
            async fn find(
                &self,
                _: &RequestContext,
                _: &str,
                _: &str,
            ) -> Result<Option<vestrace_domain::trust::SecretRef>, ApplicationError> {
                Ok(None)
            }
            async fn resolve(
                &self,
                _: &RequestContext,
                _: &vestrace_domain::trust::SecretLease,
            ) -> Result<vestrace_application::SecretMaterial, ApplicationError> {
                unreachable!()
            }
            async fn delete(
                &self,
                _: &RequestContext,
                _: vestrace_domain::SecretRefId,
            ) -> Result<(), ApplicationError> {
                Ok(())
            }
        }

        let factory = SecretBackedProviderFactory::new(
            "https://api.example.com/v1",
            Arc::new(NoSecrets),
            "openai",
        );
        let context = RequestContext::new(
            vestrace_domain::WorkspaceId::new(),
            vestrace_domain::PrincipalId::new(),
        );

        let error = match factory.provider_for(&context).await {
            Ok(_) => panic!("a provider was built with no credential"),
            Err(error) => error,
        };

        assert!(matches!(error, ApplicationError::InvalidConfiguration(_)));
        assert!(error.to_string().contains("openai"));
    }

    #[tokio::test]
    async fn the_factory_returns_the_descriptor_of_its_actual_client() {
        let context = RequestContext::new(
            vestrace_domain::WorkspaceId::new(),
            vestrace_domain::PrincipalId::new(),
        );
        let reference = vestrace_domain::trust::SecretRef::new(
            "secret://mounted/lm-studio",
            "test",
            context.workspace_id,
            PROVIDER_API_KEY_PURPOSE,
            BTreeMap::new(),
            Some("v1".into()),
        )
        .unwrap();
        let factory = SecretBackedProviderFactory::new(
            "http://localhost:12345/v1/",
            Arc::new(OneSecret { reference }),
            "lm-studio",
        );

        let resolved = factory.provider_for(&context).await.unwrap();

        assert_eq!(
            resolved.egress.endpoint(),
            "http://localhost:12345/v1/chat/completions"
        );
        assert_eq!(
            resolved.egress.destination(),
            vestrace_domain::DataDestination::LocalModel
        );
        assert!(resolved.egress.redirects_disabled());
        assert!(resolved.egress.proxy_disabled());
    }
}
