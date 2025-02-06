use crate::cloudflare::{auth::Auth, Client};
use async_trait::async_trait;
use cloudflare::{
    endpoints::cfd_tunnel::{
        create_tunnel, delete_tunnel, get_tunnel, get_tunnel_token, update_configuration,
        ConfigurationSrc, Tunnel, TunnelConfiguration, TunnelToken,
    },
    framework::response::ApiFailure,
};
use uuid::Uuid;

#[async_trait]
pub trait CloudflareTunnel: Send + Sync {
    async fn create_tunnel<'a>(
        &self,
        auth: &Auth,
        name: &str,
        tunnel_secret: Option<&'a [u8]>,
        config_src: ConfigurationSrc,
    ) -> Result<Tunnel, ApiFailure>;
    async fn delete_tunnel(&self, auth: &Auth, tunnel_id: Uuid) -> Result<(), ApiFailure>;
    async fn update_configuration(
        &self,
        auth: &Auth,
        tunnel_id: Uuid,
        config: TunnelConfiguration,
    ) -> Result<Option<TunnelConfiguration>, ApiFailure>;
    async fn get_tunnel_token(
        &self,
        auth: &Auth,
        tunnel_id: &str,
    ) -> Result<TunnelToken, ApiFailure>;
    async fn get_tunnel(&self, auth: &Auth, tunnel_id: &str) -> Result<Tunnel, ApiFailure>;
}

#[async_trait]
impl CloudflareTunnel for Client {
    async fn create_tunnel<'a>(
        &self,
        auth: &Auth,
        name: &str,
        tunnel_secret: Option<&'a [u8]>,
        config_src: ConfigurationSrc,
    ) -> Result<Tunnel, ApiFailure> {
        let params = create_tunnel::Params {
            name,
            tunnel_secret,
            config_src: &config_src,
            metadata: None,
        };

        let endpoint = create_tunnel::CreateTunnel {
            account_identifier: &auth.account_id,
            params,
        };

        match self.request(&auth.kind, &endpoint).await {
            Ok(result) => Ok(result.result),
            Err(err) => Err(err),
        }
    }

    async fn delete_tunnel(&self, auth: &Auth, tunnel_id: Uuid) -> Result<(), ApiFailure> {
        let params = delete_tunnel::Params { cascade: true };

        let tunnel_id = tunnel_id.to_string();
        let endpoint = delete_tunnel::DeleteTunnel {
            account_identifier: &auth.account_id,
            tunnel_id: &tunnel_id,
            params,
        };

        match self.request(&auth.kind, &endpoint).await {
            Ok(_) => Ok(()),
            Err(err) => Err(err),
        }
    }

    async fn update_configuration(
        &self,
        auth: &Auth,
        tunnel_id: Uuid,
        config: TunnelConfiguration,
    ) -> Result<Option<TunnelConfiguration>, ApiFailure> {
        let params = update_configuration::Params { config };

        let endpoint = update_configuration::UpdateTunnelConfiguration {
            account_identifier: &auth.account_id,
            tunnel_id,
            params,
        };

        match self.request(&auth.kind, &endpoint).await {
            Ok(res) => Ok(res.result.config),
            Err(err) => Err(err),
        }
    }

    async fn get_tunnel_token(
        &self,
        auth: &Auth,
        tunnel_id: &str,
    ) -> Result<TunnelToken, ApiFailure> {
        let endpoint = get_tunnel_token::TunnelToken {
            account_identifier: &auth.account_id,
            tunnel_id,
        };

        match self.request::<TunnelToken>(&auth.kind, &endpoint).await {
            Ok(res) => Ok(res.result),
            Err(err) => Err(err),
        }
    }

    async fn get_tunnel(&self, auth: &Auth, tunnel_id: &str) -> Result<Tunnel, ApiFailure> {
        let endpoint = get_tunnel::GetTunnel {
            account_identifier: &auth.account_id,
            tunnel_id,
        };

        match self.request::<Tunnel>(&auth.kind, &endpoint).await {
            Ok(res) => Ok(res.result),
            Err(err) => Err(err),
        }
    }
}
