use async_trait::async_trait;
use azure_core::{
    credentials::TokenCredential,
    http::{
        policies::{Policy, PolicyResult},
        Context, Request,
    },
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AuthorizationPolicy {
    token_credential: Arc<dyn TokenCredential>,
    scope: String,
}

impl AuthorizationPolicy {
    pub fn new(token_credential: Arc<dyn TokenCredential>, scope: String) -> Self {
        Self {
            token_credential,
            scope,
        }
    }
}

#[async_trait]
impl Policy for AuthorizationPolicy {
    async fn send(
        &self,
        ctx: &Context,
        request: &mut Request,
        next: &[Arc<dyn Policy>],
    ) -> PolicyResult {
        let token = self.token_credential.get_token(&[&self.scope]).await?;
        request.insert_header("authorization", format!("Bearer {}", token.token.secret()));
        next[0].send(ctx, request, &next[1..]).await
    }
}
